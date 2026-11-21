//! Incoming (decision 0011): the lists of new music, how each album relates to the library, Delete and its Undo,
//! the trash, and Lidarr. The verdicts themselves are pure (`crate::incoming`); this service reads and writes around
//! them.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;
use tokio::sync::Mutex;

use super::ToggleError;
use super::library::{Active, Library, epoch_now, label_of};
use super::shelf::{Shelf, ShelfRead, Shelves};
use crate::journal::{Entry, Event, EventId, Journal, TrackMove};
use crate::ports::{Acquisition, Files};

mod carry_on;
#[cfg(test)]
mod layout_tests;
mod queue;
mod replace;
#[cfg(test)]
pub(crate) use replace::keep_cover;
pub use queue::JobView;
pub(super) use super::incoming_base::{LidarrPair, lidarr_pair};
pub use replace::{ReplacePreview, ReplaceSheet};
#[cfg(test)]
pub(crate) use replace::{rows_back_of_copy, rows_of_copy};

/// The handler name in the journal for telling Lidarr (decision 0011 §6). Its result is what it stopped fetching.
const ACQUISITION: &str = "acquisition";
/// What the acquisition handler is about to stop fetching, recorded before it acts (plan, then act).
const ACQUISITION_PLAN: &str = "acquisition-plan";
/// The handler that gives moved files' new server items the play data of their old ones (decision 0013).
const PLAYDATA: &str = "playdata";
/// What each moved file's old item held (plays, favourites…), read before the move: the server forgets it after.
const PLAYDATA_PLAN: &str = "playdata-plan";
/// What a Copy to wrote (T1): each copy's size and time as copied, in its plan's order, so its Undo takes back only what is
/// still the copy.
const COPIED: &str = "copied";
/// Where a Copy to's Undo put the copies (T1): the second it began under, written before a file moves, and each file's trash token
/// (`trash::restore_file` brings it back), so they can be found again.
const UNCOPIED: &str = "uncopied";

/// A file's size and modification time in milliseconds, as a copy is told by (T1); none where no file is there.
fn stamp_of(path: &std::path::Path) -> Option<(u64, i64)> {
    let m = std::fs::metadata(path).ok().filter(std::fs::Metadata::is_file)?;
    let at = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis();
    Some((m.len(), i64::try_from(at).ok()?))
}

/// How a file a Copy to wrote lies when its Undo comes (T1c).
#[derive(Clone, Copy, PartialEq)]
enum Lies {
    /// In the trash already, under the second of an Undo cut short.
    Done,
    /// At its place, its size and time as copied.
    AsCopied,
    /// At its place, but not as copied: rewritten since. It refuses the Undo whole.
    Rewritten,
    /// Neither: gone from its place.
    Gone,
}

/// One user's data on one moved file's old server item, to give to its new item. Another server's: Cratarr's own item is the
/// track, which a move does not change.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PlayData {
    source: String,
    track: i64,
    old_item: String,
    /// Where the file went: its item is found by this, as a track in two folders has an item for each.
    to: (i64, String),
    user: String,
    data: serde_json::Value,
}

/// The item a moved file has at its new place, once the server has it and a sync pass linked it.
fn new_item(c: &crate::catalogue::Catalogue, p: &PlayData) -> Option<String> {
    let mut items = c.track_ids(&p.source, p.to.0, std::slice::from_ref(&p.to.1)).ok()?;
    items.pop().flatten().filter(|new| *new != p.old_item)
}

/// An album of a list as `hidden` knows it: a copy is hidden, never a record.
fn hidden_key(a: &source::Album) -> (String, String) {
    (a.id.clone(), a.library.clone().unwrap_or_default())
}

/// Whether a copy's id still reaches the server's list: `listed` is every copy on Incoming's lists. What a list no longer has
/// needs no hiding, and a copy of the record in another library says nothing of this one.
fn still_hidden(hidden: &mut std::collections::HashSet<(String, String)>, listed: &std::collections::HashSet<(String, String)>) {
    hidden.retain(|key| listed.contains(key));
}

/// What Keep would do with an album, for the page to show before asking.
#[derive(Debug, Serialize)]
pub struct KeepPreview {
    pub title: String,
    /// The album's new folder in its target library, when it can be named.
    pub folder: Option<String>,
    /// How many files move (music and others).
    pub files: usize,
    /// Why Keep cannot go ahead as it is (`needs_year`: the page asks for the year and tries again).
    pub refused: Option<KeepRefused>,
    /// Lossless files above the target's quality limit, converted first (Keep then waits in the queue).
    pub convert: usize,
    /// The layout the plan names where the library neither saved one nor follows one (PL1: none to infer): Keep is the yes that
    /// saves it as the library's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// Whether a Lidarr is set, to be told to stop fetching the album; without one it may fetch it again.
    pub acquisition: bool,
    /// A copy (C40): the album stays where it is, and the copied files are all that is written.
    pub copy: bool,
    /// The copy's own folder in its library, as this preview read it (RF10): Copy to's tap carries it back, so it takes the copy its
    /// sheet showed.
    pub from_dir: Option<String>,
}

/// What Copy to's sheet showed (RF10, the review of runs 11 and 12, RA10; the owner's "writes nothing the sheet doesn't show"): the
/// copy's folder where it goes, how many files it writes, and the copy's own folder in its library (`KeepPreview::from_dir`). Its tap
/// carries it, and is refused where the copy would now be anything else (`copying`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopySheet {
    pub folder: String,
    pub files: usize,
    pub from_dir: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum KeepRefused {
    NeedsYear,
    /// Another reason, said as is.
    Other(String),
    /// The target library's layout is still being learnt (PL1): a Keep waits for it in the queue, its words said.
    Learning(String),
}

impl From<KeepRefused> for ToggleError {
    fn from(r: KeepRefused) -> Self {
        match r {
            KeepRefused::NeedsYear => ToggleError::Refused("the album's year is not known".into()),
            KeepRefused::Other(why) | KeepRefused::Learning(why) => ToggleError::Refused(why),
        }
    }
}

/// What became of an album that left Incoming, for a page still showing it by its old id (the panel, the player's
/// View album): a server gives a moved file a new item, so a kept album has a new id once the server has it.
#[derive(Debug, Clone, PartialEq)]
pub enum AlbumFate {
    /// In its new library: its id now.
    Moved(String),
    /// In its new library, but the server has not added it yet (its title).
    NotYet(String),
    /// Deleted (its title).
    Deleted(String),
}

#[derive(Debug, Serialize)]
pub struct KeepResult {
    /// Pass to undo; `None` while queued (the job has it once done).
    pub undo: Option<String>,
    pub title: String,
    pub folder: String,
    /// Waiting for its hi-res files to be converted first.
    pub queued: bool,
}

/// The jobs a page is sent (G8): an administrator every one; a user those they queued (the conversions their own Add to waited
/// for), never another's, nor whose they are; nobody signed in none. Pure.
pub fn shown_jobs(jobs: Vec<JobView>, account: Option<&str>, admin: bool) -> Vec<JobView> {
    if admin {
        return jobs;
    }
    jobs.into_iter().filter(|j| account.is_some() && j.by.as_deref() == account).collect()
}

/// An Incoming album located on disk: the server's album, its catalogue folder, and that folder's path.
struct Located {
    /// The server it is on.
    source: String,
    album: source::Album,
    dir: IncomingDir,
    root: PathBuf,
}

/// The files of an album (`rels`, inside `dir`) that are lossless beyond `limit`: converted on the way in (decisions
/// 0013, 0096's amendment). Reads only their headers. Blocking.
fn to_convert(root: &std::path::Path, dir: &str, rels: &[String], limit: crate::convert::Limit) -> Vec<String> {
    rels.iter().filter(|rel| crate::convert::needs_conversion(&root.join(dir).join(rel), limit).ok().flatten().is_some()).cloned().collect()
}

/// Every file under `dir` (recursively), relative to it, leaving out hidden files and folders.
pub(super) fn files_under(dir: &std::path::Path) -> Vec<String> {
    fn walk(root: &std::path::Path, at: &std::path::Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(at) else { return };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else if let Ok(rel) = p.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}
use crate::settings;
use crate::source::{self, Source};

/// One source's incoming library, newest first. `supported` is false when the source cannot list albums at all.
#[derive(Debug, Clone, Serialize)]
pub struct IncomingSourceView {
    pub id: String,
    pub label: String,
    pub supported: bool,
    pub albums: Vec<source::Album>,
    pub error: Option<String>,
    /// When this list was read from the source, epoch seconds. Lists are kept and refreshed in the background,
    /// because reading one can take the source many seconds.
    pub as_of: i64,
    /// The albums Cratarr can delete: it sees their folder and can write there.
    pub deletable: Vec<String>,
    /// Per album id, what it lacks of what the library requires (the owner, 2026-10-24: marked before Keep).
    pub lacks: BTreeMap<String, Vec<&'static str>>,
    /// The libraries it was read from (the filter's library switches).
    pub libraries: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DeleteResult {
    /// Pass to undo.
    pub undo: String,
    pub title: String,
}

#[derive(Debug, Serialize)]
pub struct IncomingView {
    pub sources: Vec<IncomingSourceView>,
    /// How long a deleted album stays in the trash.
    pub trash_days: u32,
    /// The name of the library Keep and Replace put albums in (the Music library's, as its server calls it).
    pub music: Option<String>,
}

/// The used library of a server whose files lie in a catalogue folder (its id), if any.
fn library_in(a: &Active, folder: &std::path::Path) -> Option<String> {
    a.placement.libraries.iter().find(|l| a.used(&l.id) && l.folders.iter().any(|f| std::path::Path::new(f) == folder)).map(|l| l.id.clone())
}

/// The quality limit of the used library in a catalogue folder: its own (a Music library set to none keeps hi-res
/// files as they are); CD format, Music's by its use, while no library is found there.
fn quality_in(a: &Active, folder: &std::path::Path) -> settings::Quality {
    library_in(a, folder).map_or(settings::Quality::Cd, |l| a.quality_of(&l))
}

/// The catalogue's album directory behind a server path (the server's paths and Cratarr's differ in front).
fn incoming_dir<'a>(incoming: &'a [IncomingDir], path: &str) -> Option<&'a IncomingDir> {
    incoming.iter().filter(|d| !d.dir.is_empty() && path.ends_with(&format!("/{}", d.dir))).max_by_key(|d| d.dir.len())
}

/// A record's entry among Incoming's `listed`, each one library's copy: the one in library `from`. Several and none named is
/// refused: Keep, Delete and Replace land on one copy, and a record can wait in two Incoming libraries at once.
fn waiting_copy<'a>(listed: impl Iterator<Item = &'a source::Album>, album_id: &str, from: Option<&str>) -> Result<source::Album, ToggleError> {
    let mut found = listed.filter(|a| a.id == album_id && from.is_none_or(|f| a.library.as_deref() == Some(f)));
    let first = found.next().cloned().ok_or_else(|| ToggleError::Refused("that album is no longer in Incoming".into()))?;
    match (from, found.next()) {
        (None, Some(_)) => Err(ToggleError::Refused("It is in several libraries: say which copy".into())),
        _ => Ok(first),
    }
}

/// The served library as `serve::instance` has it, or in a unit test the one it gave on its thread (`served_in_tests`): what Replace's
/// guard of Music's folder reads (RF11, RY01.6 case 3), so its unit tests run it.
pub(super) fn served() -> Option<Arc<crate::serve::Serve>> {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(s) = SERVED_IN_TESTS.with(|s| s.borrow().clone()) {
        return Some(s);
    }
    crate::serve::instance()
}

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    // A unit test's served library, on its thread (`served`).
    static SERVED_IN_TESTS: std::cell::RefCell<Option<Arc<crate::serve::Serve>>> = const { std::cell::RefCell::new(None) };
}

/// A unit test's served library for `served`, on its thread; none lets it go.
#[cfg(test)]
pub(super) fn served_in_tests(s: Option<Arc<crate::serve::Serve>>) {
    SERVED_IN_TESTS.with(|t| *t.borrow_mut() = s);
}

/// Whether what lies under a copy's `dir` is that copy alone, as Delete and Move take the folder whole. `dir` is what its
/// files share, so it can be a parent of other records' folders, or the library's root: an album in two artists' folders has
/// the root for its `dir` (files A33). Blocking.
fn whole_copy(album: &str, folder: i64, root: &std::path::Path, dir: &str) -> Result<bool> {
    whole_copy_in(crate::serve::instance().as_deref(), album, folder, root, dir)
}

/// `whole_copy` as the served library `served` lists the copies (RF11: Replace's guard gives the one `served` reads); with none, the
/// folder is taken as the copy's. Blocking.
fn whole_copy_in(served: Option<&crate::serve::Serve>, album: &str, folder: i64, root: &std::path::Path, dir: &str) -> Result<bool> {
    if dir.is_empty() {
        return Ok(false);
    }
    let Some(served) = served else { return Ok(true) };
    let audio: Vec<String> = files_under(&root.join(dir)).into_iter().filter(|f| crate::catalogue::walk::is_audio(f)).map(|f| format!("{dir}/{f}")).collect();
    served.library.copy_holds_all(album, folder, dir, &audio)
}

/// The release group a copy's files name, the first in their order: what Lidarr is told to stop fetching. Blocking.
fn release_group_of(c: &crate::catalogue::Catalogue, folder: i64, dir: &str) -> Option<String> {
    let files = c.album_tags(folder, dir).ok()?;
    files.into_iter().flat_map(|(_, tags)| tags).find(|(name, value)| name == "musicbrainz_releasegroupid" && !value.trim().is_empty()).map(|(_, value)| value)
}

/// How long a folder's write test is trusted (a remount needs a restart anyway, but ownership can be fixed live).
const WRITE_TEST_SECS: i64 = 600;

/// How long the library index (for Incoming's verdicts) is kept before it is read from the catalogue again.
const INDEX_SECS: i64 = 300;

/// More than any incoming queue should hold; the page shows them all.
const INCOMING_LIMIT: usize = 1000;

/// Each library's index with its catalogue folder's id, the main libraries' folders (0101 point 1), and the other
/// folders' albums with their release groups, read together from the catalogue.
struct Owned {
    built: i64,
    main: Vec<i64>,
    dirs: Arc<Vec<IncomingDir>>,
}

/// An album directory under a catalogue folder, as the catalogue knows it.
#[derive(Debug, Clone)]
struct IncomingDir {
    /// Catalogue folder id.
    folder: i64,
    dir: String,
    release_group: Option<String>,
}

/// Album actions going on in this process, by their journal ids.
type Running = Arc<std::sync::Mutex<std::collections::HashSet<EventId>>>;

/// Album actions going on in this process on a record's copy in a library, by record and library, each with what it is doing
/// (RY01.13, RF15: `IncomingService::alone`): Replace from someone's own copy, Copy to, and their Undos, one at a time.
type Replacing = Arc<std::sync::Mutex<std::collections::HashMap<(String, i64), &'static str>>>;

/// An album action going on: its intent in the journal is no crash, and `reconcile` leaves it alone while this lives. A
/// clone goes with the blocking move: a request dropped half way ends the action's future, not its move.
#[derive(Clone)]
struct Underway(Arc<UnderwayId>);

struct UnderwayId {
    id: EventId,
    running: Running,
    _moving: crate::catalogue::CopiesMoving,
}

impl Drop for UnderwayId {
    fn drop(&mut self) {
        self.running.lock().unwrap_or_else(|p| p.into_inner()).remove(&self.id);
    }
}

impl Underway {
    fn id(&self) -> EventId {
        self.0.id
    }
}

/// An Undo of a copy going on (RF9): its entry among the actions going on (`running`), so the pass passes over its Undo begun, and a
/// second Undo of it at once is refused; given back when it ends.
struct Undoing {
    id: EventId,
    running: Running,
}

impl Drop for Undoing {
    fn drop(&mut self) {
        self.running.lock().unwrap_or_else(|p| p.into_inner()).remove(&self.id);
    }
}

/// The folders the files `rels` lay in, left empty, removed up to `root` (never `root` itself). Blocking.
fn prune_up_to(root: &std::path::Path, rels: &[String]) {
    for rel in rels {
        let mut dir = root.join(rel).parent().map(std::path::Path::to_path_buf);
        while let Some(d) = dir.filter(|d| d.as_path() != root && d.starts_with(root)) {
            if std::fs::remove_dir(&d).is_err() {
                break;
            }
            dir = d.parent().map(std::path::Path::to_path_buf);
        }
    }
}

pub struct IncomingService {
    lib: Arc<Library>,
    files: Arc<dyn Files>,
    /// Incoming lists by source and library (see `refresh`).
    cache: Mutex<std::collections::HashMap<String, IncomingSourceView>>,
    /// See `Owned`; `None` until first needed, and after settings change.
    owned: Mutex<Option<Owned>>,
    /// Optional, from Settings: asked to stop fetching albums deleted from Incoming. Swapped in whole on save.
    lidarr: std::sync::RwLock<LidarrPair>,
    /// Copies deleted here, by id and library, hidden until the server's list no longer has them.
    hidden: Mutex<std::collections::HashSet<(String, String)>>,
    /// What happened, for the handlers and for Undo (survives restarts).
    journal: Arc<Journal>,
    /// See `Underway`.
    running: Running,
    /// See `IncomingService::alone`.
    replacing: Replacing,
    /// Whether Cratarr could write in a folder, and when that was tested (epoch seconds).
    writable: Mutex<std::collections::HashMap<PathBuf, (i64, bool)>>,
    /// Saved with the sources.
    trash_days: std::sync::atomic::AtomicU32,
    /// Albums waiting for their hi-res files to be converted before Keep or Replace (survives restarts).
    queue: queue::Queue,
    /// A server's Incoming list changed (albums came or went), for pages that follow it (`/api/events`).
    lists_changed: tokio::sync::broadcast::Sender<()>,
    /// Reads the lists (shared with Collection).
    shelves: Arc<Shelves>,
}

impl IncomingService {
    pub fn new(
        lib: Arc<Library>,
        files: Arc<dyn Files>,
        journal: Arc<Journal>,
        lidarr: Option<settings::LidarrConfig>,
        trash_days: u32,
        data_dir: Option<&str>,
        shelves: Arc<Shelves>,
    ) -> Self {
        Self {
            lib,
            files,
            cache: Mutex::new(std::collections::HashMap::new()),
            owned: Mutex::new(None),
            lidarr: std::sync::RwLock::new(lidarr_pair(lidarr)),
            hidden: Mutex::new(std::collections::HashSet::new()),
            journal,
            running: Default::default(),
            replacing: Default::default(),
            writable: Mutex::new(std::collections::HashMap::new()),
            trash_days: std::sync::atomic::AtomicU32::new(trash_days),
            queue: queue::Queue::open(data_dir.map(|d| std::path::PathBuf::from(d).join("convert-queue.json"))),
            lists_changed: tokio::sync::broadcast::channel(16).0,
            shelves,
        }
    }

    /// Told when a background refresh finds a server's Incoming list changed: other albums than before.
    pub fn subscribe_incoming(&self) -> tokio::sync::broadcast::Receiver<()> {
        self.lists_changed.subscribe()
    }

    /// An action's intent written down (`Journal::intend`), and the action underway. One lock with `reconcile`'s reading
    /// of the intents, so that cannot fall between the two.
    fn intend(&self, now: i64, event: &Event) -> Result<Underway> {
        let mut running = self.running.lock().unwrap_or_else(|p| p.into_inner());
        let id = self.journal.intend(now, event)?;
        running.insert(id);
        Ok(Underway(Arc::new(UnderwayId { id, running: self.running.clone(), _moving: self.lib.catalogue().copies_moving() })))
    }

    /// Where new music comes from (the saved Lidarr), when one is set.
    fn acquisition(&self) -> Option<Arc<dyn Acquisition>> {
        let client = self.lidarr.read().unwrap_or_else(|p| p.into_inner()).1.clone();
        client.map(|l| Arc::new(l) as Arc<dyn Acquisition>)
    }

    /// The saved Lidarr connection and its client.
    pub fn lidarr_state(&self) -> LidarrPair {
        self.lidarr.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub(super) fn trash_days(&self) -> u32 {
        self.trash_days.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Whether a copy from folder `from` into folder `to` shares its blocks (C38's probe, `Files::shares`): below the floor a Want's
    /// copy is made only where it does (AL03.1). Blocking.
    pub(super) fn shares(&self, from: &std::path::Path, to: &std::path::Path) -> bool {
        self.files.shares(from, to)
    }

    /// Take saved settings into use: the Lidarr connection and the trash days.
    pub(super) fn apply_settings(&self, lidarr: LidarrPair, trash_days: u32) {
        *self.lidarr.write().unwrap_or_else(|p| p.into_inner()) = lidarr;
        self.trash_days.store(trash_days, std::sync::atomic::Ordering::Relaxed);
    }

    /// Servers and libraries may have changed: the lists are read again on the next look (or the next sync pass).
    pub async fn forget_lists(&self) {
        self.cache.lock().await.clear();
        *self.owned.lock().await = None;
    }

    /// Every server with Incoming libraries (or only `want`), their albums newest first. Read-only. Only the server is
    /// asked: the list works whether or not Cratarr can see the files.
    /// The kept lists, read from the server only when there is none yet (the background refresh keeps them fresh).
    pub async fn list(&self, want: Option<&str>) -> IncomingView {
        let mut sources = Vec::new();
        for (a, s, libraries) in self.incoming_sources() {
            if want.is_some_and(|w| w != a.cfg.id) {
                continue;
            }
            let key = format!("{}\n{}", a.cfg.id, libraries.join("\n"));
            // One read at a time: requests that find no list wait for the one reading it instead of all asking the
            // server (after a start, a page asking often would otherwise fan out to it).
            let mut cache = self.cache.lock().await;
            let view = match cache.get(&key).cloned() {
                Some(v) => v,
                None => {
                    let v = self.read_incoming(&a, &*s, &libraries).await;
                    cache.insert(key, v.clone());
                    v
                }
            };
            drop(cache);
            sources.push(view);
        }
        let hidden = self.hidden.lock().await.clone();
        for view in &mut sources {
            view.albums.retain(|a| !hidden.contains(&hidden_key(a)));
        }
        // What each album lacks, where it is Cratarr's own (its ids are the library's).
        let lacking: std::collections::HashMap<String, Vec<&'static str>> = match crate::serve::instance() {
            Some(served) => {
                let lib = served.library.clone();
                crate::run::spawn_blocking(move || lib.lacking())
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|l| (l.id.clone(), l.words()))
                    .collect()
            }
            None => Default::default(),
        };
        for view in &mut sources {
            view.lacks = view.albums.iter().filter_map(|a| Some((a.id.clone(), lacking.get(&a.id)?.clone()))).collect();
        }
        if let Some((dirs, main)) = self.owned_index().await {
            let folders = self.lib.catalogue().folders();
            let incoming: Vec<IncomingDir> = dirs.iter().filter(|d| !main.contains(&d.folder)).cloned().collect();
            for view in &mut sources {
                let mut deletable = Vec::new();
                for a in &view.albums {
                    let folder = match a.copies.first() {
                        Some(c) => folders.get(c.folder),
                        None => a.path.as_deref().and_then(|p| incoming_dir(&incoming, p)).and_then(|d| folders.get(d.folder)),
                    };
                    if let Some(f) = folder
                        && self.can_write(f).await
                    {
                        deletable.push(a.id.clone());
                    }
                }
                view.deletable = deletable;
            }
        }
        IncomingView { sources, trash_days: self.trash_days(), music: self.lib.music_name() }
    }

    /// Whether Cratarr can write in `folder`, tested at most every `WRITE_TEST_SECS`.
    async fn can_write(&self, folder: &std::path::Path) -> bool {
        let now = epoch_now();
        if let Some(&(at, ok)) = self.writable.lock().await.get(folder)
            && now - at < WRITE_TEST_SECS
        {
            return ok;
        }
        let (f, files) = (folder.to_path_buf(), self.files.clone());
        let ok = crate::run::spawn_blocking(move || files.can_write(&f)).await.unwrap_or(false);
        self.writable.lock().await.insert(folder.to_path_buf(), (now, ok));
        ok
    }

    /// Every catalogue folder's album directories with their release groups, and the main libraries' folders, read from the
    /// catalogue at most every `INDEX_SECS`: where another server's album and an Incoming copy are found by their paths.
    async fn owned_index(&self) -> Option<(Arc<Vec<IncomingDir>>, Vec<i64>)> {
        let mut owned = self.owned.lock().await;
        if let Some(o) = owned.as_ref().filter(|o| epoch_now() - o.built < INDEX_SECS) {
            return Some((o.dirs.clone(), o.main.clone()));
        }
        let (c, main) = (self.lib.catalogue(), self.lib.main_folders());
        let read = crate::run::spawn_blocking(move || -> Result<Vec<IncomingDir>> {
            let mut dirs = Vec::new();
            let ids: Vec<i64> = c.folders().ids().collect();
            for id in ids {
                dirs.extend(c.album_facts(id)?.into_iter().map(|f| IncomingDir { folder: id, dir: f.dir, release_group: f.release_group }));
            }
            Ok(dirs)
        })
        .await;
        match read {
            Ok(Ok(dirs)) => {
                let o = Owned { built: epoch_now(), main, dirs: Arc::new(dirs) };
                let out = (o.dirs.clone(), o.main.clone());
                *owned = Some(o);
                Some(out)
            }
            Ok(Err(e)) => {
                tracing::warn!("cannot read the library for Incoming: {e:#}");
                None
            }
            Err(e) => {
                tracing::warn!("cannot read the library for Incoming: {e}");
                None
            }
        }
    }

    /// Read every incoming list again and forget lists of servers or libraries no longer Incoming; a failed read
    /// keeps the previous list. Cheap: only new or changed albums cost more than one request (see the
    /// server).
    pub async fn refresh(&self) {
        // By id and library: a copy of a record leaving one Incoming library for another changes the list.
        let ids = |v: &IncomingSourceView| v.albums.iter().map(hidden_key).collect::<std::collections::BTreeSet<_>>();
        let mut keep = std::collections::HashSet::new();
        let mut changed = false;
        for (a, s, libraries) in self.incoming_sources() {
            let key = format!("{}\n{}", a.cfg.id, libraries.join("\n"));
            let fresh = self.read_incoming(&a, &*s, &libraries).await;
            let mut cache = self.cache.lock().await;
            if fresh.error.is_none() || !cache.contains_key(&key) {
                changed |= cache.get(&key).is_none_or(|old| ids(old) != ids(&fresh));
                cache.insert(key.clone(), fresh);
            }
            keep.insert(key);
        }
        {
            let mut cache = self.cache.lock().await;
            let before = cache.len();
            cache.retain(|k, _| keep.contains(k));
            changed |= cache.len() != before;
        }
        if changed {
            let _ = self.lists_changed.send(());
        }
        // A deleted album stays hidden until the server's fresh list no longer has it (or Undo brings it back).
        let listed: std::collections::HashSet<(String, String)> = self.cache.lock().await.values().flat_map(|v| v.albums.iter().map(hidden_key)).collect();
        still_hidden(&mut *self.hidden.lock().await, &listed);
    }

    /// An action on a copy undone: it is listed again. With its library not known any more, every copy of the record is.
    async fn unhide(&self, source_id: &str, album_id: &str, folder: i64) {
        let library = self.library_at(source_id, folder);
        self.hidden.lock().await.retain(|(id, l)| id != album_id || library.as_ref().is_some_and(|at| at != l));
    }

    /// Delete an Incoming album: into the trash inside its folder (Undo brings it back; emptied after
    /// the trash days set in Settings), and Lidarr stops fetching it.
    pub async fn delete(&self, source_id: &str, album_id: &str, from: Option<&str>) -> Result<DeleteResult, ToggleError> {
        let album = self.waiting(source_id, album_id, from).await?;
        self.delete_listed(source_id, album).await
    }

    /// A record's entry on Incoming's lists: its copy in library `from` (`waiting_copy`).
    async fn waiting(&self, source_id: &str, album_id: &str, from: Option<&str>) -> Result<source::Album, ToggleError> {
        let cache = self.cache.lock().await;
        waiting_copy(cache.values().filter(|v| v.id == source_id).flat_map(|v| v.albums.iter()), album_id, from)
    }

    /// What each catalogue folder's library is (`Library::folder_kinds`).
    pub fn folder_kinds(&self) -> Vec<(i64, crate::settings::LibraryKind)> {
        self.lib.folder_kinds()
    }

    /// Each catalogue folder's tag rules (`Library::folder_rules`).
    pub fn folder_rules(&self) -> crate::catalogue::FolderRules {
        self.lib.folder_rules()
    }

    /// The catalogue folders in use that are no Incoming library's: the kept ones, where an original holds its copies' bytes (RF5,
    /// as the backup's list keeps them).
    fn not_incoming(&self) -> Vec<i64> {
        let incoming: Vec<i64> = self.lib.folder_kinds().into_iter().filter(|(_, k)| *k == settings::LibraryKind::Incoming).map(|(f, _)| f).collect();
        self.lib.catalogue().folders().ids().filter(|f| !incoming.contains(f)).collect()
    }

    /// The catalogue folders that libraries switched off (`source/library`) leave out: those no library switched on
    /// keeps (`Library::folders_off`), for which copy a stream hears.
    pub fn folders_off(&self, without: &[String]) -> Vec<i64> {
        self.lib.folders_off(without)
    }

    /// The kept libraries whose albums can be deleted, by server and library id: Cratarr writes in their folder (each
    /// library's trash is its own; 0101 point 1: main libraries are equal). Whose to delete they are is the view's.
    pub(super) async fn kept_deletable(&self) -> std::collections::HashSet<(String, String)> {
        let mut out = std::collections::HashSet::new();
        for a in self.lib.live().iter() {
            for id in &a.kept() {
                let folder = a.placement.libraries.iter().find(|l| &l.id == id).and_then(|l| l.folders.first());
                if let Some(f) = folder
                    && self.can_write(std::path::Path::new(f)).await
                {
                    out.insert((a.cfg.id.clone(), id.clone()));
                }
            }
        }
        out
    }

    /// Delete a listed album of any used library (decision 0034): into that library's own trash (each library keeps
    /// its own, the owner, 2026-10-16), Undo for the trash days.
    pub async fn delete_listed(&self, source_id: &str, album: source::Album) -> Result<DeleteResult, ToggleError> {
        let at = self.locate_listed(source_id, album).await?;
        self.refuse_if_queued(&at.album.id, at.dir.folder)?;
        let (album, dir, folder) = (at.album, at.dir, at.root);
        let catalogue = self.lib.catalogue();
        if !self.can_write(&folder).await {
            return Err(ToggleError::Refused("Cratarr cannot write in that library".into()));
        }
        // Write-ahead: the journal knows what is about to happen before it does (the token is known up front), so a
        // crash in between is finished or forgotten by `reconcile`, never lost. Lidarr hears of it once applied, and only
        // for an album that leaves an Incoming folder (the main libraries stay out of it).
        let now = epoch_now();
        let token = crate::trash::token(now, &dir.dir);
        let event = Event::AlbumRejected {
            source: source_id.to_string(),
            album_id: album.id.clone(),
            title: album.title.clone(),
            folder: dir.folder,
            dir: dir.dir.clone(),
            token: token.clone(),
            release_group: if self.lib.is_kept(dir.folder) { None } else { dir.release_group.clone() },
        };
        // Deleting the album that plays: the player stops first (a player going on with an item the server then drops
        // reports on it, which Jellyfin refuses; decision 0013). Nothing is played after: it is gone. Playing another copy
        // of the record, it goes on.
        let (c, f, d) = (catalogue.clone(), dir.folder, dir.dir.clone());
        let (tracks, leaving): (Vec<i64>, Vec<(i64, String)>) = crate::run::spawn_blocking(move || {
            c.album_tags(f, &d).unwrap_or_default().into_iter().filter_map(|(rel, _)| Some((c.track_of(f, &rel)?, (f, rel)))).unzip()
        })
        .await
        .unwrap_or_default();
        if let Some(mut p) = self.playing_among(&tracks, &tracks.iter().map(|t| (*t, *t)).collect::<Vec<_>>(), &leaving).await {
            self.hold(&mut p).await;
        }
        let underway = self.intend(now, &event)?;
        let id = underway.id();
        let (f, rel, files, held) = (folder.clone(), dir.dir.clone(), self.files.clone(), underway.clone());
        let trashed = crate::run::spawn_blocking(move || {
            let _held = held;
            files.trash(&f, &rel, now)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"));
        if let Err(e) = trashed.and_then(|r| r) {
            self.journal.forget(id)?;
            return Err(ToggleError::Refused(format!("{e:#}")));
        }
        self.finish_reject(&event).await;
        // Its record's Placed Soulseek items (V1, U3): none stands for a copy any more, nor keeps the record's next request from
        // being searched, though no round reads its folder again. Every Delete of an Incoming copy comes here: App's, and
        // Incoming's own endpoint, an administrator's.
        if !self.lib.is_kept(dir.folder)
            && let Some(record) = crate::serve::ids::record_number(&album.id)
        {
            super::soulseek::forget_placed(record);
        }
        self.journal.applied(id)?;
        self.hidden.lock().await.insert(hidden_key(&album));
        catalogue.poke();
        Ok(DeleteResult { undo: id.to_string(), title: album.title })
    }

    /// After the files went to the trash: the catalogue rows follow (their tracks keep their ids, decision 0011: Undo
    /// brings them back unchanged). Safe to repeat.
    async fn finish_reject(&self, event: &Event) {
        let Event::AlbumRejected { folder, dir, token, title, .. } = event else { return };
        let (c, f, rel, tok) = (self.lib.catalogue(), *folder, dir.clone(), token.clone());
        if let Err(e) = crate::run::spawn_blocking(move || c.mark_trashed(f, &rel, &tok)).await.map_err(|e| anyhow::anyhow!("{e}")).and_then(|r| r) {
            tracing::warn!("catalogue: could not mark {title} as trashed: {e:#}");
        }
    }

    /// Actions the journal intended but does not know to have happened (a crash in between, or a request dropped): what
    /// the disk shows decides. Done → finish the bookkeeping and mark it applied (the handlers then run); not done → forget
    /// it. This runs every minute, so an action going on now is passed over (`Underway`): a Keep caught before its first
    /// file arrives would lose its event, and with it its Undo, Lidarr's word and its play data.
    pub(super) async fn reconcile(&self) {
        let intents = {
            let running = self.running.lock().unwrap_or_else(|p| p.into_inner());
            self.journal.intents().map(|i| i.into_iter().filter(|e| !running.contains(&e.id)).collect::<Vec<_>>())
        };
        let intents = match intents {
            Ok(i) => i,
            Err(e) => {
                tracing::warn!("journal: {e:#}");
                return;
            }
        };
        for entry in intents {
            let happened = match &entry.event {
                Event::AlbumRejected { folder, token, .. } => {
                    let root = self.lib.catalogue().folders().get(*folder).cloned();
                    root.is_some_and(|r| crate::trash::holds(&r, token))
                }
                // Keep: it happened when any file reached its target; then it is finished forward (the moves repeat
                // safely).
                Event::AlbumAdopted { moves, .. } => {
                    let folders = self.lib.catalogue().folders();
                    moves.iter().any(|m| folders.get(m.to.0).is_some_and(|f| f.join(&m.to.1).is_file()))
                }
                // Replace: it happened when the old files' trash entry exists (they move first).
                Event::AlbumReplaced { from_dir, replaced, token, .. } => {
                    let trash = replaced.first().map_or(from_dir.0, |r| r.to.0);
                    let root = self.lib.catalogue().folders().get(trash).cloned();
                    root.is_some_and(|r| crate::trash::holds(&r, token))
                }
                // Copy to (C40): it happened when any copy reached its target; then it is finished forward (the copies repeat
                // safely).
                Event::AlbumCopied { copies, .. } => {
                    let folders = self.lib.catalogue().folders();
                    copies.iter().any(|m| folders.get(m.to.0).is_some_and(|f| f.join(&m.to.1).is_file()))
                }
                // Replace from someone's own copy (RY01): it happened when the old files' trash entry exists (they move first); then
                // it is finished forward, or put back whole where its copy cannot be made.
                Event::AlbumReplacedFromCopy { to_dir, token, .. } => {
                    let root = self.lib.catalogue().folders().get(to_dir.0).cloned();
                    root.is_some_and(|r| crate::trash::holds(&r, token))
                }
            };
            let (title, _) = entry.event.album();
            if happened
                && matches!(entry.event, Event::AlbumCopied { .. })
                && let Err((_, e)) = self.finish_copy(entry.id, &entry.event, None, None).await
            {
                tracing::warn!("could not finish copying {title}: {e:#}");
                continue;
            }
            if happened && matches!(entry.event, Event::AlbumReplacedFromCopy { .. }) {
                match self.finish_from_copy(entry.id, &entry.event, false, None, None).await {
                    Ok(()) => {}
                    Err((true, e)) => {
                        tracing::warn!("could not finish replacing {title}: {e:#}");
                        continue;
                    }
                    Err((false, e)) => {
                        tracing::warn!("put {title} back as it was: its copy could not be made whole: {e:#}");
                        if let Err(e) = self.journal.forget(entry.id) {
                            tracing::warn!("journal: {e:#}");
                        }
                        continue;
                    }
                }
            }
            if happened && matches!(entry.event, Event::AlbumAdopted { .. } | Event::AlbumReplaced { .. }) {
                let moving = self.lib.catalogue().copies_moving();
                if let Some(steps) = self.steps(&entry.event) {
                    let (files, held) = (self.files.clone(), moving.clone());
                    let done = crate::run::spawn_blocking(move || {
                        let _held = held;
                        files.move_all(&steps)
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"));
                    if let Err(e) = done.and_then(|r| r) {
                        tracing::warn!("could not finish keeping {title}: {e:#}");
                        continue;
                    }
                }
                self.finish_adopt(&entry.event, false, false).await;
                self.finish_replace(&entry.event, false, false).await;
            }
            let result = if happened {
                self.finish_reject(&entry.event).await;
                tracing::info!("finished an interrupted action on {title}");
                self.journal.applied(entry.id)
            } else {
                // Stopped on its first copy: what that left must not refuse the same Keep tried again.
                if let Some(steps) = self.steps(&entry.event) {
                    let files = self.files.clone();
                    let _ = crate::run::spawn_blocking(move || files.clear_parts(&steps)).await;
                }
                tracing::info!("forgot an action on {title} that never happened");
                self.journal.forget(entry.id)
            };
            if let Err(e) = result {
                tracing::warn!("journal: {e:#}");
            }
        }
        // Undos of copies cut short (RF9).
        self.finish_undos().await;
    }

    /// Undos of copies begun and not finished (RF9, the review of runs 11 and 12, RA7: a stop between taking the copies and the end),
    /// as the disk shows them, as for an action's intent: where a copy lies in the trash under the Undo's second, it is finished as a
    /// second tap would finish it, with no tap; one of which nothing happened is forgotten. One that cannot be finished puts back what
    /// it took (`put_back_undo`): never a short album, never a main library with neither copy. An Undo going on now is passed over.
    async fn finish_undos(&self) {
        let begun = match self.journal.begun(UNCOPIED) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("journal: {e:#}");
                return;
            }
        };
        for entry in begun {
            let (Event::AlbumCopied { title, to_dir, copies, .. } | Event::AlbumReplacedFromCopy { title, to_dir, copies, .. }) = &entry.event else { continue };
            let (Some(stamp), Some(root)) = (self.undo_begun(entry.id), self.lib.catalogue().folders().get(to_dir.0).cloned()) else { continue };
            if self.running.lock().unwrap_or_else(|p| p.into_inner()).contains(&entry.id) {
                continue;
            }
            let rels: Vec<String> = copies.iter().map(|m| m.to.1.clone()).collect();
            let happened = crate::run::spawn_blocking(move || rels.iter().any(|rel| root.join(crate::trash::TRASH_DIR).join(crate::trash::token(stamp, rel)).is_file()))
                .await
                .unwrap_or(true);
            if !happened {
                tracing::info!("forgot an Undo of {title} that never happened");
                if let Err(e) = self.journal.forget_result(entry.id, UNCOPIED) {
                    tracing::warn!("journal: {e:#}");
                }
                continue;
            }
            let done = match &entry.event {
                Event::AlbumCopied { .. } => self.undo_copy(&entry).await,
                _ => self.undo_from_copy(&entry).await,
            };
            match done {
                Ok(_) => tracing::info!("finished an Undo of {title} cut short"),
                Err(e) => tracing::warn!("could not finish an Undo of {title} cut short: {e}"),
            }
        }
    }

    /// The second an Undo of a copy begun on entry `id` puts its files under (T1, RF9), kept on the entry before a file moves; none
    /// where none was begun.
    fn undo_begun(&self, id: EventId) -> Option<i64> {
        self.journal.result(id, UNCOPIED).ok().flatten().and_then(|v| v["stamp"].as_i64())
    }

    /// The Undo of entry `id` going on (RF9), until what this gives goes: refused while another Undo of it goes on (a tap's, or the
    /// pass finishing one cut short).
    fn undoing(&self, id: EventId, title: &str) -> Result<Undoing, ToggleError> {
        if !self.running.lock().unwrap_or_else(|p| p.into_inner()).insert(id) {
            return Err(ToggleError::Refused(format!("{title} is being undone")));
        }
        Ok(Undoing { id, running: self.running.clone() })
    }

    /// An Undo of a copy that cannot go on, put back (RF9): each copy it took into the trash under its second (`stamp`) back at its
    /// place, and for a Replace from someone's own copy each old file it brought back into the trash again (the copy of its name went
    /// before it), all or none; its second forgotten, so the album is as it was before the Undo: never short, and never a main library
    /// with neither copy. Where that cannot be, an error is logged and its second kept, for the pass to try again. Its rows never
    /// moved: an Undo's rows follow once its files all stand.
    async fn put_back_undo(&self, entry: &Entry, stamp: i64) {
        let (title, _) = entry.event.album();
        let (folder, copies, replaced) = match &entry.event {
            Event::AlbumCopied { to_dir, copies, .. } => (to_dir.0, copies.clone(), Vec::new()),
            Event::AlbumReplacedFromCopy { to_dir, copies, replaced, .. } => (to_dir.0, copies.clone(), replaced.clone()),
            _ => return,
        };
        let Some(root) = self.lib.catalogue().folders().get(folder).cloned() else { return };
        let files = self.files.clone();
        let back = crate::run::spawn_blocking(move || -> Result<()> {
            let at = |rel: &str| (root.clone(), rel.to_string());
            let trashed = |rel: &str| format!("{}/{}", crate::trash::TRASH_DIR, crate::trash::token(stamp, rel));
            let took = |rel: &str| root.join(trashed(rel)).is_file();
            // What the Undo did, as its plan: the copies into the trash, then each old file back where the copy of its name went first.
            let mut steps: Vec<crate::moves::Step> =
                copies.iter().filter(|m| took(&m.to.1)).map(|m| crate::moves::Step { from: at(&m.to.1), to: at(&trashed(&m.to.1)) }).collect();
            steps.extend(
                replaced
                    .iter()
                    .filter(|r| !root.join(&r.to.1).exists() && root.join(&r.from.1).is_file() && copies.iter().filter(|m| m.to.1 == r.from.1).all(|m| took(&m.to.1)))
                    .map(|r| crate::moves::Step { from: at(&r.to.1), to: at(&r.from.1) }),
            );
            files.move_back(&steps)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
        .and_then(|done| done)
        .and_then(|()| self.journal.forget_result(entry.id, UNCOPIED));
        match back {
            Ok(()) => tracing::info!("put back what the Undo of {title} took: as it was before"),
            Err(e) => tracing::error!("could not put back what the Undo of {title} took, so it stays for the next pass: {e:#}"),
        }
    }

    /// Who did the album action an undo key names (X5): its account, as the journal has it; none for one not known.
    pub fn done_by(&self, key: &str) -> Option<String> {
        self.journal.account(key.parse().ok()?).ok().flatten()
    }

    /// Tests elsewhere: the journal Incoming writes to.
    #[cfg(any(test, feature = "test-support"))]
    pub fn journal_for_tests(&self) -> &Journal {
        &self.journal
    }

    /// Undo an album action (Delete, Keep or Replace) by its undo key. Returns the album's title.
    pub async fn undo(&self, key: &str) -> Result<String, ToggleError> {
        let gone = || ToggleError::Refused("that can no longer be undone".into());
        let id: EventId = key.parse().map_err(|_| gone())?;
        let entry = self.journal.get(id)?.filter(|e| e.applied).ok_or_else(gone)?;
        match &entry.event {
            Event::AlbumRejected { .. } => self.undo_delete(key).await,
            Event::AlbumAdopted { .. } => self.undo_keep(&entry).await,
            Event::AlbumReplaced { .. } => self.undo_replace(&entry).await,
            Event::AlbumCopied { .. } => self.undo_copy(&entry).await,
            Event::AlbumReplacedFromCopy { .. } => self.undo_from_copy(&entry).await,
        }
    }

    /// Undo a Delete: back from the trash, its tracks keep their ids, and Lidarr fetches what it fetched before.
    pub async fn undo_delete(&self, key: &str) -> Result<String, ToggleError> {
        let gone = || ToggleError::Refused("that deletion can no longer be undone".into());
        let id: EventId = key.parse().map_err(|_| gone())?;
        let entry = self.journal.get(id)?.filter(|e| e.applied).ok_or_else(gone)?;
        let Event::AlbumRejected { source, album_id, title, folder, dir, token, .. } = &entry.event else { return Err(gone()) };
        if entry.undone {
            return Err(ToggleError::Refused(format!("{title} is already back")));
        }
        let catalogue = self.lib.catalogue();
        let root = catalogue
            .folders()
            .get(*folder)
            .cloned()
            .ok_or_else(|| ToggleError::Refused("that Incoming folder is not in use any more".into()))?;
        let (t, files) = (token.clone(), self.files.clone());
        crate::run::spawn_blocking(move || files.restore(&root, &t))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
        let (c, f, rel, tok) = (catalogue.clone(), *folder, dir.clone(), token.clone());
        if let Err(e) = crate::run::spawn_blocking(move || c.unmark_trashed(f, &rel, &tok)).await.map_err(|e| anyhow::anyhow!("{e}")).and_then(|r| r) {
            tracing::warn!("catalogue: could not bring {title} back from the trash: {e:#}");
        }
        // The requests the Delete closed open again (Y4), their arrival back, before Lidarr fetches again: an open request's arrival
        // is what stops it (X1). Before `mark_undone`, after which a handler still telling Lidarr puts it back too.
        self.reopen_requests(id).await;
        self.journal.mark_undone(id, epoch_now())?;
        self.resume_fetching(id, title).await;
        self.unhide(source, album_id, *folder).await;
        catalogue.poke();
        Ok(title.clone())
    }

    /// Undo a Copy to (C40): file by file, as Keep's (T1), each copied file into its library's trash (Q13: as every Undo), its row
    /// with it, marked trashed as a Delete marks them (the token kept on the entry); the folders it leaves empty go, never the
    /// library's own. A file added to the copy's folder since stays, and so does a later album at its place: with none of the copies
    /// there as copied it is refused. One copied file rewritten since, its size or time not as copied (an entry from before T1: as its
    /// original's now), refuses it whole, naming the file, before anything moves (T1c, as OQ-D for Replace): it never leaves the album
    /// short, nor takes a file that isn't as copied; deleting the copy is the way then. Refused whole too where no original in a kept
    /// library holds a copy's bytes any more (RF5): the copy is the album's only copy then. Its second goes on the entry before a file
    /// moves, and its files move as one plan, all or none (RF9): an Undo cut short is finished by the next tap or by the pass, the
    /// files already in the trash under it counting as done, never as rewritten; one that cannot go on puts back what it took
    /// (`put_back_undo`), so the album is never left short. One Undo of an entry at a time (`undoing`). The original as it was.
    async fn undo_copy(&self, entry: &Entry) -> Result<String, ToggleError> {
        let gone = || ToggleError::Refused("that copy can no longer be undone".into());
        let Event::AlbumCopied { album_id, title, to_dir, copies, .. } = &entry.event else { return Err(gone()) };
        if entry.undone {
            return Err(ToggleError::Refused(format!("{title} is no longer copied")));
        }
        let _undoing = self.undoing(entry.id, title)?;
        let _alone = self.alone(album_id, to_dir.0, title, "being undone")?;
        let catalogue = self.lib.catalogue();
        // The copies held on their records while its files move, to its end (RF15, as Keep's Undo holds them).
        let moving = catalogue.copies_moving();
        let folders = catalogue.folders();
        let root = folders.get(to_dir.0).cloned().ok_or_else(|| ToggleError::Refused("that library is not in use any more".into()))?;
        let made: Vec<Option<(u64, i64)>> = self.journal.result(entry.id, COPIED).ok().flatten().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
        let trashed_at = |at: i64, rel: &str| root.join(crate::trash::TRASH_DIR).join(crate::trash::token(at, rel));
        let begun = self.undo_begun(entry.id);
        let plan: Vec<(String, Option<(u64, i64)>)> = copies
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let was = match made.get(i) {
                    Some(made) => *made,
                    None => folders.get(m.from.0).and_then(|f| stamp_of(&f.join(&m.from.1))),
                };
                (m.to.1.clone(), was)
            })
            .collect();
        // How each copied file lies, before anything moves.
        let (r, p) = (root.clone(), plan.clone());
        let lies = crate::run::spawn_blocking(move || -> Vec<Lies> {
            p.iter()
                .map(|(rel, was)| {
                    let here = r.join(rel);
                    if begun.is_some_and(|at| !here.exists() && r.join(crate::trash::TRASH_DIR).join(crate::trash::token(at, rel)).is_file()) {
                        Lies::Done
                    } else if was.is_some() && stamp_of(&here) == *was {
                        Lies::AsCopied
                    } else if here.exists() {
                        Lies::Rewritten
                    } else {
                        Lies::Gone
                    }
                })
                .collect()
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut refused = None;
        if !lies.iter().any(|l| matches!(l, Lies::Done | Lies::AsCopied)) {
            refused = Some(format!("None of the files copied of {title} is there as it was copied"));
        } else if let Some(((rel, _), _)) = plan.iter().zip(&lies).find(|(_, l)| **l == Lies::Rewritten) {
            let name = rel.strip_prefix(&format!("{}/", to_dir.1)).unwrap_or(rel.as_str());
            refused = Some(format!("{name} has changed since {title} was copied: delete the copy instead"));
        } else {
            // RF5: a copy whose bytes no original holds any more (deleted, its trash emptied, rewritten, gone with its library) is the
            // album's only copy: never into the trash by this Undo.
            let taking: Vec<String> = plan.iter().zip(&lies).filter(|(_, l)| **l == Lies::AsCopied).map(|((rel, _), _)| rel.clone()).collect();
            let (c, folder, kept) = (catalogue.clone(), to_dir.0, self.not_incoming());
            let only = crate::run::spawn_blocking(move || c.only_copies(folder, &taking, &kept)).await.map_err(|e| anyhow::anyhow!("{e}"))??;
            if !only.is_empty() {
                refused = Some("The original is gone: this is the album's only copy.".to_string());
            }
        }
        if let Some(why) = refused {
            // An Undo cut short that cannot go on puts back what it took (RF9).
            if let Some(stamp) = begun {
                self.put_back_undo(entry, stamp).await;
            }
            return Err(ToggleError::Refused(why));
        }
        // The second its files go under: an Undo begun before keeps its own; a new one takes one no copy's name is taken in.
        let stamp = match begun {
            Some(at) => at,
            None => {
                let mut at = epoch_now();
                while copies.iter().any(|m| trashed_at(at, &m.to.1).exists()) {
                    at += 1;
                }
                self.journal.done(entry.id, UNCOPIED, epoch_now(), &serde_json::json!({ "stamp": at, "tokens": [] }))?;
                at
            }
        };
        // Each copy still as copied into the trash under that second, as one plan: all or none (RF9). One that fails puts back what it
        // took, so the album is whole again.
        let at = |rel: &str| (root.clone(), rel.to_string());
        let steps: Vec<crate::moves::Step> = plan
            .iter()
            .zip(&lies)
            .filter(|(_, l)| **l == Lies::AsCopied)
            .map(|((rel, _), _)| crate::moves::Step { from: at(rel), to: at(&format!("{}/{}", crate::trash::TRASH_DIR, crate::trash::token(stamp, rel))) })
            .collect();
        let taken: Vec<String> = plan.iter().zip(&lies).filter(|(_, l)| matches!(l, Lies::Done | Lies::AsCopied)).map(|((rel, _), _)| rel.clone()).collect();
        let (r, files, rels, held) = (root.clone(), self.files.clone(), plan.iter().map(|(rel, _)| rel.clone()).collect::<Vec<_>>(), moving.clone());
        let moved = crate::run::spawn_blocking(move || -> Result<()> {
            let _held = held;
            files.move_all(&steps)?;
            prune_up_to(&r, &rels);
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
        .and_then(|done| done);
        if let Err(e) = moved {
            self.put_back_undo(entry, stamp).await;
            return Err(ToggleError::Refused(format!("{e:#}")));
        }
        // Their rows into the trash with them, marked trashed as a Delete marks them; one that fails leaves the Undo to the pass.
        let (c, folder) = (catalogue.clone(), to_dir.0);
        let tokens = crate::run::spawn_blocking(move || -> Result<Vec<String>> {
            let mut tokens = Vec::new();
            for rel in &taken {
                let token = crate::trash::token(stamp, rel);
                if c.has_row(folder, rel) {
                    c.mark_trashed(folder, rel, &token)?;
                }
                tokens.push(token);
            }
            Ok(tokens)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
        let now = epoch_now();
        self.journal.done(entry.id, UNCOPIED, now, &serde_json::json!({ "stamp": stamp, "tokens": tokens }))?;
        self.journal.mark_undone(entry.id, now)?;
        catalogue.poke();
        // The backup's list of shared copies, anew (C41).
        super::backup_list::refresh();
        Ok(title.clone())
    }

    /// Find a listed album on disk: of Cratarr's own the copy it names, where it says it lies; another server's by its path.
    async fn locate_listed(&self, source_id: &str, album: source::Album) -> Result<Located, ToggleError> {
        let Some((folder, dir)) = album.copies.first().map(|c| (c.folder, c.dir.clone())) else { return self.locate_by_path(source_id, album).await };
        let root = self.root_of(folder)?;
        let (c, id, at, d) = (self.lib.catalogue(), album.id.clone(), root.clone(), dir.clone());
        let found = crate::run::spawn_blocking(move || -> Result<Option<Option<String>>> { Ok(whole_copy(&id, folder, &at, &d)?.then(|| release_group_of(&c, folder, &d))) })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))??;
        let release_group = found.ok_or_else(|| ToggleError::Refused("Cratarr cannot see that album's folder".into()))?;
        Ok(Located { source: source_id.to_string(), album, dir: IncomingDir { folder, dir, release_group }, root })
    }

    /// Find another server's album on disk by the end of its path, in its own library's folder where the listing says
    /// which: two libraries can each hold a folder of that name.
    async fn locate_by_path(&self, source_id: &str, album: source::Album) -> Result<Located, ToggleError> {
        let path = album.path.clone().ok_or_else(|| ToggleError::Refused("the server gives no folder for that album".into()))?;
        let (dirs, main) = self.owned_index().await.ok_or_else(|| ToggleError::Refused("the catalogue is not ready yet".into()))?;
        let own = album.library.as_deref().and_then(|l| {
            let live = self.lib.live();
            self.folder_of(live.iter().find(|a| a.cfg.id == source_id)?, l).ok()
        });
        let at = |in_main: bool| -> Vec<IncomingDir> { dirs.iter().filter(|d| main.contains(&d.folder) == in_main && own.is_none_or(|o| o == d.folder)).cloned().collect() };
        let (incoming, in_main) = (at(false), at(true));
        let dir = incoming_dir(&incoming, &path)
            .or_else(|| incoming_dir(&in_main, &path))
            .cloned()
            .ok_or_else(|| ToggleError::Refused("Cratarr cannot see that album's folder".into()))?;
        let root = self.lib.catalogue().folders().get(dir.folder).cloned().ok_or_else(|| ToggleError::Refused("that folder is not in use any more".into()))?;
        Ok(Located { source: source_id.to_string(), album, dir, root })
    }

    /// Find an Incoming album on disk: its copy in library `from`.
    async fn locate(&self, source_id: &str, album_id: &str, from: Option<&str>) -> Result<Located, ToggleError> {
        let album = self.waiting(source_id, album_id, from).await?;
        self.locate_listed(source_id, album).await
    }

    /// How Keep and Replace name an album in the main library at `target` (PL1): its layout's template, and whether the plan names
    /// one the library neither saved nor follows (none to infer: the default, asked); while it is pending, what a placing waits with.
    pub(super) fn placing_layout(&self, source_id: &str, target: i64) -> Result<(crate::naming::Template, bool), String> {
        use crate::app::library::Layout;
        match self.lib.layout_now(target) {
            Layout::Saved(t) | Layout::Inferred(t) => Ok((t, false)),
            Layout::NoneToInfer => Ok((crate::naming::Template::default(), true)),
            Layout::Pending => Err(crate::app::library::learning_words(&self.library_name(source_id, target))),
        }
    }

    /// The layout a plan into `target` names and asks the administrator's yes for (PL1: none to infer), as Settings writes it.
    pub(super) fn asked_layout(&self, source_id: &str, target: i64) -> Option<String> {
        self.lib.is_kept(target).then(|| self.placing_layout(source_id, target).ok()).flatten().filter(|(_, asks)| *asks).map(|(t, _)| t.to_string())
    }

    /// Work out Keep for an album into the main library in catalogue folder `target`: its new names, or why not. Reads
    /// the disk and the catalogue; changes nothing.
    async fn plan_keep(&self, at: &Located, target: i64, year: Option<&str>) -> Result<(PathBuf, Result<crate::incoming::KeepPlan, KeepRefused>), ToggleError> {
        let music = self.root_of(target)?;
        let name = self.library_name(&at.source, target);
        let template = match self.placing_layout(&at.source, target) {
            Ok((t, _)) => t,
            Err(waits) => return Ok((music, Err(KeepRefused::Learning(waits)))),
        };
        let (c, folder, dir, root, music_, year) = (self.lib.catalogue(), at.dir.folder, at.dir.dir.clone(), at.root.clone(), music.clone(), year.map(str::to_string));
        let plan = crate::run::spawn_blocking(move || -> Result<Result<crate::incoming::KeepPlan, KeepRefused>> {
            let prefix = format!("{dir}/");
            let music_files: Vec<(String, crate::naming::Tags)> = c
                .album_tags(folder, &dir)?
                .into_iter()
                .map(|(rel, tags)| (rel.strip_prefix(&prefix).unwrap_or(&rel).to_string(), crate::naming::Tags(tags)))
                .collect();
            let others: Vec<String> =
                files_under(&root.join(&dir)).into_iter().filter(|f| !music_files.iter().any(|(m, _)| m == f)).collect();
            let plan = crate::incoming::plan_keep(&template, &music_files, &others, year.as_deref())
                .map(|p| crate::incoming::follow_spelling(p, |parent| crate::incoming::folders_in(&music_, parent)));
            Ok(match plan {
                Ok(plan) if music_.join(&plan.folder).exists() => Err(KeepRefused::Other(format!("{name} already has a folder named {}", plan.folder))),
                Ok(plan) => Ok(plan),
                Err(crate::incoming::KeepRefusal::NeedsYear) => Err(KeepRefused::NeedsYear),
                Err(e) => Err(KeepRefused::Other(e.to_string())),
            })
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        Ok((music, plan))
    }

    /// What Keep would do with an album (decision 0013) into `to` (`keep_target`), for the page to show before asking.
    /// Changes nothing.
    pub async fn keep_preview(&self, source_id: &str, album_id: &str, from: Option<&str>, to: Option<&str>, year: Option<&str>) -> Result<KeepPreview, ToggleError> {
        let at = self.locate(source_id, album_id, from).await?;
        let (target, quality) = self.keep_target(source_id, to)?;
        let (_, plan) = self.plan_keep(&at, target, year).await?;
        let convert = self.to_convert(&at, quality).await.len();
        let acquisition = self.acquisition().is_some();
        let layout = self.asked_layout(source_id, target);
        Ok(match plan {
            Ok(p) => KeepPreview { title: at.album.title, folder: Some(p.folder), files: p.moves.len(), refused: None, convert, layout, acquisition, copy: false, from_dir: Some(at.dir.dir.clone()) },
            Err(r) => KeepPreview { title: at.album.title, folder: None, files: 0, refused: Some(r), convert, layout: None, acquisition, copy: false, from_dir: Some(at.dir.dir.clone()) },
        })
    }

    /// The album's music files that need converting first, under the target's quality limit (none: no file).
    async fn to_convert(&self, at: &Located, quality: settings::Quality) -> Vec<String> {
        let Some(limit) = quality.limit() else { return Vec::new() };
        let (c, folder, dir, root) = (self.lib.catalogue(), at.dir.folder, at.dir.dir.clone(), at.root.clone());
        crate::run::spawn_blocking(move || {
            let prefix = format!("{dir}/");
            let rels: Vec<String> = c
                .album_tags(folder, &dir)
                .unwrap_or_default()
                .into_iter()
                .map(|(rel, _)| rel.strip_prefix(&prefix).unwrap_or(&rel).to_string())
                .collect();
            to_convert(&root, &dir, &rels, limit)
        })
        .await
        .unwrap_or_default()
    }

    /// Keep (decision 0013): move a new album from Incoming into a main library (`to`, else the server's first; 0101
    /// point 2), named per the library's template. Write-ahead like Delete; the files and catalogue rows move (track
    /// ids kept); Lidarr stops fetching it, and the moved files' new server items get the old ones' play data
    /// (handlers). Undo puts it all back.
    /// Lossless files above the target's quality limit are converted first, in the queue: the plan is checked now, so
    /// a refusal shows at once.
    pub async fn keep(&self, source_id: &str, album_id: &str, from: Option<&str>, to: Option<&str>, year: Option<&str>) -> Result<KeepResult, ToggleError> {
        let at = self.locate(source_id, album_id, from).await?;
        self.refuse_if_queued(album_id, at.dir.folder)?;
        // None named (Home Assistant's promote): where the page puts it by default (X5).
        let to = match to {
            Some(t) => Some(t.to_string()),
            None => self.default_target(source_id, &at.album).await?,
        };
        let (target, quality) = self.keep_target(source_id, to.as_deref())?;
        self.move_located(source_id, at, target, quality, year).await
    }

    /// Where Keep puts an album it is given no library for (X5: Home Assistant's promote): where the page puts it by default
    /// (`albums::default_target`), its requester's own music made now where this is its first use.
    async fn default_target(&self, source_id: &str, album: &source::Album) -> Result<Option<String>, ToggleError> {
        let served = crate::serve::instance();
        let requester = match served.clone() {
            Some(s) => {
                let asked = crate::run::spawn_blocking(move || super::requests::requesters(&s)).await.unwrap_or_default();
                super::albums::requester_of(&asked, album).cloned()
            }
            None => None,
        };
        let on = |r: &String| served.as_ref().is_none_or(|s| s.users.get(r).is_some_and(|u| u.active));
        let to = {
            let live = self.lib.live();
            let Some(a) = live.iter().find(|a| a.cfg.id == source_id) else { return Ok(None) };
            match super::albums::default_target(a, album.library.as_deref(), requester.filter(on).as_deref()) {
                Some(t) if !a.used(&t) => Err(t),
                other => Ok(other),
            }
        };
        match to {
            Ok(to) => Ok(to),
            Err(own) => {
                let account = std::path::Path::new(&own).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.lib.own_library(&account).await.map_err(ToggleError::Upstream)
            }
        }
    }

    /// Keep's target (0101 point 2): `to`, a main library of the server, else its first main library (the page's
    /// `keep_to`); as `target_of` has it.
    fn keep_target(&self, source_id: &str, to: Option<&str>) -> Result<(i64, settings::Quality), ToggleError> {
        let live = self.lib.live();
        let a = live.iter().find(|a| a.cfg.id == source_id).ok_or_else(|| ToggleError::Refused("that server is not connected".into()))?;
        let (main, kept) = (a.main(), a.kept());
        let to = match to {
            Some(t) if kept.iter().any(|m| m == t) => t,
            Some(_) => return Err(ToggleError::Refused("An album is added to a main library or someone's own music".into())),
            None => main.first().map(String::as_str).ok_or_else(|| ToggleError::Refused("no main library is set".into()))?,
        };
        self.target_of(source_id, to)
    }

    /// The quality limit of a server's used library in catalogue folder `folder`, as a move into it has it
    /// (`target_of`): Replace's target's.
    fn quality_at(&self, source_id: &str, folder: i64) -> settings::Quality {
        let (live, folders) = (self.lib.live(), self.lib.catalogue().folders());
        match (live.iter().find(|a| a.cfg.id == source_id), folders.get(folder)) {
            (Some(a), Some(f)) => quality_in(a, f),
            _ => settings::Quality::Cd,
        }
    }

    /// The catalogue's folder `folder`, as a root on disk.
    fn root_of(&self, folder: i64) -> Result<PathBuf, ToggleError> {
        self.lib.catalogue().folders().get(folder).cloned().ok_or_else(|| ToggleError::Refused("that library's folder is not in use".into()))
    }

    /// What a move of `title` from catalogue folder `from` into `to` is called on Health's list of changes, and its
    /// title there: a keep into a main library, by its name (0101 point 2), else a move.
    pub(super) fn adopted_named(&self, source_id: &str, from: i64, to: i64, title: &str) -> (&'static str, String) {
        if self.lib.is_kept(to) && !self.lib.is_kept(from) {
            ("keep", format!("Added to {}: {title}", self.library_name(source_id, to)))
        } else {
            ("move", format!("Moved: {title}"))
        }
    }

    /// A Copy to's task, as `app::tasks` lists it (C40).
    const COPY_TASK: &'static str = "copy";

    /// A Replace from someone's own copy's task (RY01).
    const REPLACE_TASK: &'static str = "replace";

    /// What a Replace from someone's own copy is called on Health's list of changes to files, and its title (RY01): the record, the
    /// main library it went into, and whose music it came from.
    pub(super) fn replaced_from_named(&self, source_id: &str, from: i64, to: i64, title: &str) -> (&'static str, String) {
        ("replace", format!("Replaced in {} from {}: {title}", self.library_name(source_id, to), self.library_name(source_id, from)))
    }

    /// What a Save to is called on Health's list of changes to files, and its title (C40; F33: "Saved to", its kind still `copy`).
    pub(super) fn copied_named(&self, source_id: &str, to: i64, title: &str) -> (&'static str, String) {
        ("copy", format!("Saved to {}: {title}", self.library_name(source_id, to)))
    }

    /// The id of a server's used library in catalogue folder `folder`.
    fn library_at(&self, source_id: &str, folder: i64) -> Option<String> {
        let (live, folders) = (self.lib.live(), self.lib.catalogue().folders());
        library_in(live.iter().find(|a| a.cfg.id == source_id)?, folders.get(folder)?)
    }

    /// What a server calls its used library in catalogue folder `folder`, for what Keep, Move and Replace say; else
    /// "that library".
    pub fn library_name(&self, source_id: &str, folder: i64) -> String {
        let (live, folders) = (self.lib.live(), self.lib.catalogue().folders());
        let named = live.iter().find(|a| a.cfg.id == source_id).and_then(|a| {
            let id = library_in(a, folders.get(folder)?)?;
            a.placement.libraries.iter().find(|l| l.id == id).map(|l| l.name.clone())
        });
        named.unwrap_or_else(|| "that library".into())
    }

    /// Move (decision 0034): an album of any used library into another of its server; Keep is the move into a main
    /// library. The target's policies apply on the way in: its naming (a main library: the template; another library:
    /// the album's folder as it is) and its quality limit (lossless files above it converted first, in the queue).
    /// `album` is the listed album (a main library's or another library's).
    pub async fn move_album(&self, source_id: &str, album: source::Album, to: &str, year: Option<&str>) -> Result<KeepResult, ToggleError> {
        let at = self.locate_listed(source_id, album).await?;
        self.refuse_if_queued(&at.album.id, at.dir.folder)?;
        let (target, quality) = self.destination_of(source_id, to)?;
        if target == at.dir.folder {
            return Err(ToggleError::Refused("the album is in that library already".into()));
        }
        self.move_located(source_id, at, target, quality, year).await
    }

    /// What a move would do (decision 0034), for the page to show before asking. Changes nothing.
    pub async fn move_preview(&self, source_id: &str, album: source::Album, to: &str, year: Option<&str>) -> Result<KeepPreview, ToggleError> {
        let at = self.locate_listed(source_id, album).await?;
        let (target, quality) = self.destination_of(source_id, to)?;
        let (_, plan) = self.plan_move(&at, target, year).await?;
        let convert = self.to_convert(&at, quality).await.len();
        let acquisition = self.acquisition().is_some() && !self.lib.is_kept(at.dir.folder);
        let layout = self.asked_layout(source_id, target);
        Ok(match plan {
            Ok(p) => KeepPreview { title: at.album.title, folder: Some(p.folder), files: p.moves.len(), refused: None, convert, layout, acquisition, copy: false, from_dir: Some(at.dir.dir.clone()) },
            Err(r) => KeepPreview { title: at.album.title, folder: None, files: 0, refused: Some(r), convert, layout: None, acquisition, copy: false, from_dir: Some(at.dir.dir.clone()) },
        })
    }

    /// Copy to (C40): a listed copy of Cratarr's own copied into another used library of its server (someone's own music), the
    /// original staying where it is: one album, two copies on the same tracks. An album action like Keep (the owner, 2026-11-17):
    /// named as the target names its albums (`plan_move`), copied as it is whatever the target's quality limit (OQ4 (a): shared, and
    /// free), told on Health's list of changes as it goes, one Undo into the target's trash. Only the copied files are written:
    /// no cover or tag into or beside them, nor into the original (files.md CP2), nothing carried on, Lidarr not told.
    pub async fn copy_album(&self, source_id: &str, album: source::Album, to: &str, year: Option<&str>, sheet: Option<&CopySheet>) -> Result<KeepResult, ToggleError> {
        let at = self.locate_listed(source_id, album).await?;
        self.refuse_if_queued(&at.album.id, at.dir.folder)?;
        let (target, _) = self.destination_of(source_id, to)?;
        if target == at.dir.folder {
            return Err(ToggleError::Refused("the album is in that library already".into()));
        }
        let (what, title) = self.copied_named(source_id, target, &at.album.title);
        let run = super::file_runs::track(&self.journal, what, title);
        let done = self.copying(&at, target, year, sheet).await;
        match &done {
            Ok(k) => run.moved(k.undo.clone(), self.read_in(&at, target)),
            Err(e) => run.failed(&at.album.id, e.to_string()),
        }
        done
    }

    /// What a Copy to would do (C40), for the page to show before asking: as a move's, said to be a copy, nothing converted, Lidarr
    /// not told. Changes nothing.
    pub async fn copy_preview(&self, source_id: &str, album: source::Album, to: &str, year: Option<&str>) -> Result<KeepPreview, ToggleError> {
        let at = self.locate_listed(source_id, album).await?;
        let (target, _) = self.destination_of(source_id, to)?;
        let (_, plan) = self.plan_move(&at, target, year).await?;
        Ok(match plan {
            Ok(p) => KeepPreview { title: at.album.title, folder: Some(p.folder), files: p.moves.len(), refused: None, convert: 0, layout: None, acquisition: false, copy: true, from_dir: Some(at.dir.dir.clone()) },
            Err(r) => KeepPreview { title: at.album.title, folder: None, files: 0, refused: Some(r), convert: 0, layout: None, acquisition: false, copy: true, from_dir: Some(at.dir.dir.clone()) },
        })
    }

    /// Copy to's copying (`copy_album`): its plan, its intent in the journal first (`AlbumCopied`), the copies and their rows, then
    /// applied. What a failure made goes, and the intent is forgotten. One at a time of a record into a library, and none while an
    /// Undo or a Replace of its copy there goes on (RF15: `alone`).
    async fn copying(&self, at: &Located, target: i64, year: Option<&str>, sheet: Option<&CopySheet>) -> Result<KeepResult, ToggleError> {
        let _alone = self.alone(&at.album.id, target, &at.album.title, "being copied")?;
        let (root, plan) = self.plan_move(at, target, year).await?;
        let plan = plan.map_err(ToggleError::from)?;
        // The tap carries what its sheet showed (RF10): a copy that would now be another, or written elsewhere or with more or fewer
        // files, is refused with nothing written.
        if let Some(s) = sheet
            && (s.folder != plan.folder || s.files != plan.moves.len() || s.from_dir != at.dir.dir)
        {
            return Err(ToggleError::Refused(format!("Changed since you looked. {} would not be copied as its sheet showed: look again.", at.album.title)));
        }
        if !self.can_write(&root).await {
            return Err(ToggleError::Refused("Cratarr cannot write in that library".into()));
        }
        let c = self.lib.catalogue();
        let from = |rel: &str| format!("{}/{rel}", at.dir.dir);
        let copies: Vec<TrackMove> =
            plan.moves.iter().map(|(rel, to)| TrackMove { track: c.track_of(at.dir.folder, &from(rel)), from: (at.dir.folder, from(rel)), to: (target, to.clone()), was: None }).collect();
        let event = Event::AlbumCopied {
            source: at.source.clone(),
            album_id: at.album.id.clone(),
            title: at.album.title.clone(),
            from_dir: (at.dir.folder, at.dir.dir.clone()),
            to_dir: (target, plan.folder.clone()),
            copies,
        };
        let underway = self.intend(epoch_now(), &event)?;
        let id = underway.id();
        // Its progress where long work shows it (the owner's light rule for 0.17: `app::tasks`), as its account's: its files to
        // copy, then done. A second copy of the account's at once copies all the same, with no task of its own.
        let (files, doing) = (plan.moves.len() as u64, format!("Copying {}", at.album.title));
        let task = crate::serve::who::current().and_then(|w| crate::runner::runner().begin(Self::COPY_TASK, &w.user, false).ok());
        if let Some(t) = &task {
            t.step(&doing, 0, Some(files));
        }
        let told = task.as_ref().map(|t| (t.stepper(), doing.clone(), files));
        let copied = self.finish_copy(id, &event, Some(underway.clone()), told).await;
        if let Some(t) = task {
            if copied.is_ok() {
                t.step(&doing, files, Some(files));
            }
            t.finish(copied.as_ref().map(|_| ()).map_err(|(_, e)| anyhow::anyhow!("{e:#}")));
        }
        match copied {
            Ok(()) => {}
            // The copies stand (T4): the intent stays, for the pass to finish (`reconcile`) and the entry to have its Undo.
            Err((true, e)) => return Err(ToggleError::Refused(format!("{e:#}"))),
            Err((false, e)) => {
                self.journal.forget(id)?;
                return Err(ToggleError::Refused(format!("{e:#}")));
            }
        }
        self.journal.applied(id)?;
        c.poke();
        // The backup's list of shared copies, anew (C41).
        super::backup_list::refresh();
        Ok(KeepResult { undo: Some(id.to_string()), title: at.album.title.clone(), folder: plan.folder, queued: false })
    }

    /// A Copy to's files and rows (C40), safe to repeat: every file copied (`Files::copy_all`: all or none, the original's time
    /// kept), then the copies' rows made from the originals' (`Catalogue::copy_rows`), each shared as its copy is. `held`: the
    /// action's mark as under way, kept through it. A failure says whether the copies were written by then (T4).
    async fn finish_copy(&self, id: EventId, event: &Event, held: Option<Underway>, told: Option<(crate::runner::Stepper, String, u64)>) -> Result<(), (bool, anyhow::Error)> {
        let Event::AlbumCopied { copies, .. } = event else { return Ok(()) };
        let (before, after) = (|e: anyhow::Error| (false, e), |e: anyhow::Error| (true, e));
        let steps = self.steps(event).ok_or_else(|| before(anyhow::anyhow!("the folders are not in use any more")))?;
        let files = self.files.clone();
        let (shared, made) = crate::run::spawn_blocking(move || -> anyhow::Result<(Vec<bool>, Vec<Option<(u64, i64)>>)> {
            let _held = held;
            // Each file counted as it lands (RN's TU2, RA8), where a task follows it.
            let shared = files.copy_all_told(&steps, &|n| {
                if let Some((s, doing, of)) = &told {
                    s.step(doing, n as u64, Some(*of));
                }
            })?;
            Ok((shared, steps.iter().map(|s| stamp_of(&s.to.0.join(&s.to.1))).collect()))
        })
        .await
        .map_err(|e| before(anyhow::anyhow!("{e}")))?
        .map_err(before)?;
        // What it wrote, as its Undo will find it (T1).
        self.journal.done(id, COPIED, epoch_now(), &serde_json::json!(made)).map_err(after)?;
        let rows: Vec<((i64, String), (i64, String), bool)> = copies.iter().zip(shared).map(|(m, s)| (m.from.clone(), m.to.clone(), s)).collect();
        let c = self.lib.catalogue();
        crate::run::spawn_blocking(move || c.copy_rows(&rows)).await.map_err(|e| after(anyhow::anyhow!("{e}")))?.map_err(after)?;
        Ok(())
    }

    async fn move_located(&self, source_id: &str, at: Located, target: i64, quality: settings::Quality, year: Option<&str>) -> Result<KeepResult, ToggleError> {
        let asked = self.asked_layout(source_id, target);
        let (root, plan) = self.plan_move(&at, target, year).await?;
        // The library's layout still being learnt (PL1): the Keep waits in the queue until it is known, never placed by a guess.
        let plan = match plan {
            Err(KeepRefused::Learning(waits)) => return Ok(self.enqueue_waiting(source_id, at, queue::Action::Keep, year, target, waits)),
            plan => plan.map_err(ToggleError::from)?,
        };
        // None to infer: the plan named the default, and this Keep is the administrator's yes to it, saved as the library's own.
        if let Some(t) = asked.as_deref().and_then(|t| t.parse::<crate::naming::Template>().ok()) {
            self.lib.save_layout(target, &t);
        }
        if !self.can_write(&at.root).await || !self.can_write(&root).await {
            return Err(ToggleError::Refused("Cratarr cannot write in both libraries".into()));
        }
        let convert = self.to_convert(&at, quality).await;
        if let (false, Some(limit)) = (convert.is_empty(), quality.limit()) {
            return Ok(self.enqueue(source_id, at, queue::Action::Keep, year, convert, limit, plan.folder, target));
        }
        self.keep_now(&at, target, year, Vec::new(), None).await
    }

    /// A Move's or a Save to's target (F34, the owner 2026-11-19): a main library or someone's own music, never an Incoming library,
    /// which its fetcher alone fills; refused at the preview and again at the tap, so a library made Incoming after its sheet was shown
    /// is refused there.
    fn destination_of(&self, source_id: &str, library: &str) -> Result<(i64, settings::Quality), ToggleError> {
        let (target, quality) = self.target_of(source_id, library)?;
        if !self.lib.is_kept(target) {
            return Err(ToggleError::Refused(INCOMING_IS_NO_DESTINATION.into()));
        }
        Ok((target, quality))
    }

    /// A library of a server as Cratarr's catalogue folder (a main library's too: its own, 0101 point 1), and its
    /// quality limit.
    fn target_of(&self, source_id: &str, library: &str) -> Result<(i64, settings::Quality), ToggleError> {
        let live = self.lib.live();
        let a = live.iter().find(|a| a.cfg.id == source_id).ok_or_else(|| ToggleError::Refused("that server is not connected".into()))?;
        if !a.used(library) {
            return Err(ToggleError::Refused("that library is not used".into()));
        }
        Ok((self.folder_of(a, library)?, a.quality_of(library)))
    }

    /// The catalogue folder a library of a server lies in (its first folder).
    fn folder_of(&self, a: &Active, library: &str) -> Result<i64, ToggleError> {
        let placed = a.placement.libraries.iter().find(|l| l.id == library).and_then(|l| l.folders.first()).ok_or_else(|| ToggleError::Refused("Cratarr cannot see that library's files".into()))?;
        self.lib.catalogue().folders().id_of(std::path::Path::new(placed)).ok_or_else(|| ToggleError::Refused("that library's folder is not in use yet".into()))
    }

    /// Work out a move's names, or why not: into a main library by the template (Keep's plan), into another library as
    /// the album's folder is. Reads the disk and the catalogue; changes nothing.
    async fn plan_move(&self, at: &Located, target: i64, year: Option<&str>) -> Result<(PathBuf, Result<crate::incoming::KeepPlan, KeepRefused>), ToggleError> {
        if self.lib.is_kept(target) {
            return self.plan_keep(at, target, year).await;
        }
        let root = self.root_of(target)?;
        let (from, dir, to_root) = (at.root.clone(), at.dir.dir.clone(), root.clone());
        let plan = crate::run::spawn_blocking(move || {
            if to_root.join(&dir).exists() {
                return Err(KeepRefused::Other(format!("that library already has a folder named {dir}")));
            }
            let moves = files_under(&from.join(&dir)).into_iter().map(|rel| (rel.clone(), format!("{dir}/{rel}"))).collect();
            Ok(crate::incoming::KeepPlan { folder: dir, moves })
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok((root, plan))
    }

    /// Keep, its files ready, told on Health's list of changes to files as it goes (decision 0092's one list; the owner,
    /// 2026-11-02: a move "was hard to see"): moving, then adding until the server has read the album, then done.
    async fn keep_now(
        &self,
        at: &Located,
        target: i64,
        year: Option<&str>,
        converted: Vec<crate::journal::Converted>,
        job: Option<u64>,
    ) -> Result<KeepResult, ToggleError> {
        let (what, title) = self.adopted_named(&at.source, at.dir.folder, target, &at.album.title);
        let run = super::file_runs::track(&self.journal, what, title);
        let done = self.keep_moving(at, target, year, converted, job).await;
        match &done {
            Ok(k) => run.moved(k.undo.clone(), self.read_in(at, target)),
            Err(e) => run.failed(&at.album.id, e.to_string()),
        }
        done
    }

    /// What says a copy that went into catalogue folder `target` has been read there, for Health's "adding". Cratarr's own:
    /// the record has a copy there, as its ids tell nothing, and a copy that does not serve has no link to tell by (files
    /// B5). Another server: its tracks' new items name an album.
    fn read_in(&self, at: &Located, target: i64) -> impl Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send>> + Send + 'static {
        let (lib, journal, source, album, own) = (self.lib.clone(), self.journal.clone(), at.source.clone(), at.album.id.clone(), !at.album.copies.is_empty());
        move || {
            let (lib, journal, source, album) = (lib.clone(), journal.clone(), source.clone(), album.clone());
            Box::pin(async move {
                match own {
                    true => copy_dir_in(&album, target).await.is_some(),
                    false => matches!(fate_of(&lib, &journal, &source, &album).await, Some(AlbumFate::Moved(_))),
                }
            })
        }
    }

    /// Keep's moving (`converted`: what conversion did just before). `job`: the queued job doing it, told the event's
    /// id as soon as it is intended.
    async fn keep_moving(
        &self,
        at: &Located,
        target: i64,
        year: Option<&str>,
        converted: Vec<crate::journal::Converted>,
        job: Option<u64>,
    ) -> Result<KeepResult, ToggleError> {
        // Nothing put inside or beside the files (COU.12, GEN.58): an arrival is added as it arrived, its picture the store's
        // resolver's to show.
        let (root, plan) = self.plan_move(at, target, year).await?;
        let plan = plan.map_err(ToggleError::from)?;
        if !self.can_write(&at.root).await || !self.can_write(&root).await {
            return Err(ToggleError::Refused("Cratarr cannot write in both libraries".into()));
        }
        let c = self.lib.catalogue();
        let from = |rel: &str| format!("{}/{rel}", at.dir.dir);
        let moves: Vec<TrackMove> = plan
            .moves
            .iter()
            .map(|(rel, to)| TrackMove { track: c.track_of(at.dir.folder, &from(rel)), from: (at.dir.folder, from(rel)), to: (target, to.clone()), was: None })
            .collect();
        // Before anything moves: what users did with the old server items (the server forgets them after). Cratarr's own item
        // is the track, which stays.
        let mut play: Vec<PlayData> = Vec::new();
        for (source, s) in self.lib.connected_sources().into_iter().filter(|(source, _)| source != crate::source::cratarr::KIND.id) {
            let rels: Vec<String> = moves.iter().map(|m| m.from.1.clone()).collect();
            let items = c.track_ids(&source, at.dir.folder, &rels).unwrap_or_default();
            for (m, item) in moves.iter().zip(items) {
                let (Some(track), Some(item)) = (m.track, item) else { continue };
                match s.user_data(&item).await {
                    Ok(users) => play.extend(users.into_iter().map(|(user, data)| PlayData {
                        source: source.clone(),
                        track,
                        old_item: item.clone(),
                        to: m.to.clone(),
                        user,
                        data,
                    })),
                    Err(e) => tracing::warn!("{source}: could not read the play data of {item}: {e:#}"),
                }
            }
        }
        let event = Event::AlbumAdopted {
            source: at.source.clone(),
            album_id: at.album.id.clone(),
            title: at.album.title.clone(),
            from_dir: (at.dir.folder, at.dir.dir.clone()),
            to_dir: (target, plan.folder.clone()),
            moves,
            // Lidarr stops fetching an album that leaves a folder it fetches into: an Incoming one (the main libraries
            // stay out of it); with no release group it is not asked.
            release_group: if self.lib.is_kept(at.dir.folder) { None } else { at.dir.release_group.clone() },
            converted,
        };
        let now = epoch_now();
        let underway = self.intend(now, &event)?;
        let id = underway.id();
        if let Some(job) = job {
            self.queue.intended(job, id);
        }
        self.journal.done(id, PLAYDATA_PLAN, now, &serde_json::to_value(&play).map_err(anyhow::Error::from)?)?;
        // Playing from this album: carried on with its new copy after the move (the old items go).
        let (order, leaving): (Vec<i64>, Vec<(i64, String)>) = match &event {
            Event::AlbumAdopted { moves, .. } => moves.iter().filter_map(|m| Some((m.track?, m.from.clone()))).unzip(),
            _ => Default::default(),
        };
        let mut playing = self.playing_among(&order, &order.iter().map(|t| (*t, *t)).collect::<Vec<_>>(), &leaving).await;
        if let Some(p) = playing.as_mut() {
            self.hold(p).await;
        }
        let steps = self.steps(&event).ok_or_else(|| ToggleError::Refused("the folders are not in use any more".into()))?;
        let (files, held) = (self.files.clone(), underway.clone());
        let moved = crate::run::spawn_blocking(move || {
            let _held = held;
            files.move_all(&steps)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"));
        if let Err(e) = moved.and_then(|r| r) {
            self.journal.forget(id)?;
            return Err(ToggleError::Refused(format!("{e:#}")));
        }
        self.finish_adopt(&event, false, playing.is_some()).await;
        self.journal.applied(id)?;
        self.deliver_requests(id, &at.album.id).await;
        self.hidden.lock().await.insert(hidden_key(&at.album));
        *self.owned.lock().await = None;
        c.poke();
        if let Some(p) = playing {
            carry_on::carry_on(self.lib.clone(), p, &event);
        }
        Ok(KeepResult { undo: Some(id.to_string()), title: at.album.title.clone(), folder: plan.folder, queued: false })
    }

    /// What became of a server's album after Keep, Replace or Delete; `None` when none of them happened to it.
    pub async fn album_fate(&self, source_id: &str, album_id: &str) -> Option<AlbumFate> {
        fate_of(&self.lib, &self.journal, source_id, album_id).await
    }

    /// Refused while this copy waits in the conversion queue (Cancel it first). A record's other copies are free.
    fn refuse_if_queued(&self, album_id: &str, folder: i64) -> Result<(), ToggleError> {
        match self.queue.busy(album_id, folder) {
            true => Err(ToggleError::Refused("that album is being converted".into())),
            false => Ok(()),
        }
    }

    /// The moves that undo an album action, in order: its own moves back, then any conversion undone (the converted
    /// files into the trash, the originals back from it). One plan, so all of it happens or none. A converted file goes
    /// to the trash from where the action took it, never by way of its old place: `moves::apply` looks for every file
    /// before anything moves, so a step from a place only an earlier step fills is refused, every time. `None` when a
    /// folder is no longer in use.
    fn undo_steps(&self, event: &Event) -> Option<Vec<crate::moves::Step>> {
        let mut back: Vec<crate::moves::Step> =
            self.steps(event)?.into_iter().rev().map(|s| crate::moves::Step { from: s.to, to: s.from }).collect();
        let (Event::AlbumAdopted { from_dir, converted, .. } | Event::AlbumReplaced { from_dir, converted, .. }) = event else {
            return Some(back);
        };
        let root = self.lib.catalogue().folders().get(from_dir.0)?.clone();
        let at = |rel: &str| (root.clone(), rel.to_string());
        for c in converted {
            let trash = at(&queue::converted_trash(c)?);
            match back.iter_mut().find(|s| s.to == at(&c.converted)) {
                Some(step) => step.to = trash,
                // No step took it away: it is where it was converted.
                None => back.push(crate::moves::Step { from: at(&c.converted), to: trash }),
            }
        }
        for c in converted {
            back.push(crate::moves::Step { from: at(&c.trashed), to: at(&c.original) });
        }
        Some(back)
    }

    /// After an Undo moved them back: the rows of converted files that were renamed (a WAV became FLAC) go back to
    /// the originals' names, track ids kept. Safe to repeat.
    async fn unconvert_rows(&self, event: &Event) {
        let (Event::AlbumAdopted { from_dir, converted, .. } | Event::AlbumReplaced { from_dir, converted, .. }) = event else { return };
        let (c, folder, converted) = (self.lib.catalogue(), from_dir.0, converted.clone());
        let done = crate::run::spawn_blocking(move || -> Result<()> {
            for v in converted.iter().filter(|v| v.converted != v.original && !c.has_row(folder, &v.original)) {
                c.move_files(folder, &v.converted, folder, &v.original)?;
            }
            c.changed_outside_pass();
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
        .and_then(|r| r);
        if let Err(e) = done {
            tracing::warn!("catalogue: could not put the originals' rows back: {e:#}");
        }
    }

    /// The file moves of an album action, as paths on disk, in order (Replace: the old files out first); `None` when
    /// a folder is no longer in use.
    fn steps(&self, event: &Event) -> Option<Vec<crate::moves::Step>> {
        let folders = self.lib.catalogue().folders();
        let moves: Vec<&TrackMove> = match event {
            Event::AlbumAdopted { moves, .. } => moves.iter().collect(),
            Event::AlbumReplaced { moves, replaced, .. } => replaced.iter().chain(moves).collect(),
            Event::AlbumCopied { copies, .. } => copies.iter().collect(),
            Event::AlbumReplacedFromCopy { copies, replaced, .. } => replaced.iter().chain(copies).collect(),
            _ => return Some(Vec::new()),
        };
        moves
            .iter()
            .map(|m| {
                Some(crate::moves::Step {
                    from: (folders.get(m.from.0)?.clone(), m.from.1.clone()),
                    to: (folders.get(m.to.0)?.clone(), m.to.1.clone()),
                })
            })
            .collect()
    }

    /// After the files moved (or moved back, `back`): the catalogue rows follow, track ids kept, and the servers hear
    /// of it at once. Safe to repeat.
    /// `hold_gone`: tell the servers only where the files went; where they left is told later (carry-on, when the album
    /// plays: its player must not go on with an item the server already dropped).
    async fn finish_adopt(&self, event: &Event, back: bool, hold_gone: bool) {
        let Event::AlbumAdopted { moves, title, .. } = event else { return };
        let c = self.lib.catalogue();
        let pairs: Vec<((i64, String), (i64, String))> =
            moves.iter().map(|m| if back { (m.to.clone(), m.from.clone()) } else { (m.from.clone(), m.to.clone()) }).collect();
        let (c2, pairs2) = (c.clone(), pairs.clone());
        let moved = crate::run::spawn_blocking(move || -> Result<()> {
            for ((ff, fr), (tf, tr)) in &pairs2 {
                c2.move_files(*ff, fr, *tf, tr)?;
            }
            c2.changed_outside_pass();
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
        .and_then(|r| r);
        if let Err(e) = moved {
            tracing::warn!("catalogue: could not follow the files of {title}: {e:#}");
        }
        tell_moved(&self.lib, title, &pairs, if hold_gone { Told::Came } else { Told::Both }).await;
    }


    /// Undo a Keep: the files back into Incoming, the catalogue rows with them, Lidarr fetching as before.
    async fn undo_keep(&self, entry: &Entry) -> Result<String, ToggleError> {
        let Event::AlbumAdopted { source, album_id, title, from_dir, .. } = &entry.event else { unreachable!() };
        if entry.undone {
            return Err(ToggleError::Refused(format!("{title} is already back where it was")));
        }
        let steps = self.undo_steps(&entry.event).ok_or_else(|| ToggleError::Refused("the folders are not in use any more".into()))?;
        let moving = self.lib.catalogue().copies_moving();
        let (files, held) = (self.files.clone(), moving.clone());
        crate::run::spawn_blocking(move || {
            let _held = held;
            files.move_all(&steps)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
        self.finish_adopt(&entry.event, true, false).await;
        self.unconvert_rows(&entry.event).await;
        self.undeliver_requests(entry.id).await;
        self.journal.mark_undone(entry.id, epoch_now())?;
        self.resume_fetching(entry.id, title).await;
        self.unhide(source, album_id, from_dir.0).await;
        *self.owned.lock().await = None;
        self.lib.catalogue().poke();
        Ok(title.clone())
    }

    /// Give the moved files' new server items the play data their old items had (read before the move). Waits until
    /// the server has the new items and Cratarr has linked them (a scan, then a sync pass); safe to repeat. A plan from
    /// before these knew where their file went reads as nothing, and so does one of Cratarr's own, which could only wait.
    async fn carry_play_data(&self, entry: &Entry) {
        if self.journal.result(entry.id, PLAYDATA).ok().flatten().is_some() {
            return;
        }
        let plan: Vec<PlayData> =
            self.journal.result(entry.id, PLAYDATA_PLAN).ok().flatten().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
        let c = self.lib.catalogue();
        let sources: std::collections::HashMap<String, Arc<dyn Source>> = self.lib.connected_sources().into_iter().collect();
        let mut ready = Vec::new();
        for p in &plan {
            let Some(s) = sources.get(&p.source) else { return };
            // Not scanned or linked yet: the next pass tries again.
            let Some(new) = new_item(&c, p) else { return };
            ready.push((s.clone(), new, p));
        }
        for (s, new, p) in ready {
            if let Err(e) = s.set_user_data(&new, &p.user, &p.data).await {
                tracing::warn!("{}: could not carry play data to {new}: {e:#}", p.source);
                return;
            }
        }
        if let Err(e) = self.journal.done(entry.id, PLAYDATA, epoch_now(), &serde_json::json!(plan.len())) {
            tracing::warn!("journal: {e:#}");
        }
    }

    /// React to a journal event (decision 0011 §6). Safe to call twice: what is done is recorded per event.
    pub(super) async fn handle(&self, entry: &Entry) {
        if matches!(entry.event, Event::AlbumRejected { .. }) {
            // Nothing moved, so no play data to carry.
            if self.journal.result(entry.id, PLAYDATA).ok().flatten().is_none() {
                let _ = self.journal.done(entry.id, PLAYDATA, epoch_now(), &serde_json::json!(0));
            }
        } else {
            self.carry_play_data(entry).await;
        }
        if self.journal.result(entry.id, ACQUISITION).ok().flatten().is_some() {
            return;
        }
        // Deleted, kept or replaced: Lidarr stops fetching the album either way (else it fetches it again). Best effort:
        // the action stands; when Lidarr cannot be told, the next pass tries again. Plan, then act: what will be
        // stopped is recorded first, so a retry after a crash stops (and Undo resumes) exactly that, instead of reading
        // a state it already changed.
        let (title, release_group) = entry.event.album();
        let stopped: Vec<i64> = match (self.acquisition(), release_group) {
            (Some(l), Some(rg)) => {
                let planned: Option<Vec<i64>> =
                    self.journal.result(entry.id, ACQUISITION_PLAN).ok().flatten().and_then(|v| serde_json::from_value(v).ok());
                let plan = match planned {
                    Some(p) => p,
                    None => match l.fetching(rg).await {
                        Ok(ids) => {
                            if let Err(e) = self.journal.done(entry.id, ACQUISITION_PLAN, epoch_now(), &serde_json::json!(ids)) {
                                tracing::warn!("journal: {e:#}");
                                return;
                            }
                            ids
                        }
                        Err(e) => {
                            tracing::warn!("Lidarr: could not ask about {title}: {e:#}");
                            return;
                        }
                    },
                };
                if let Err(e) = l.set_fetching(&plan, false).await {
                    tracing::warn!("Lidarr: could not stop fetching {title}: {e:#}");
                    return;
                }
                plan
            }
            _ => Vec::new(),
        };
        if let Err(e) = self.journal.done(entry.id, ACQUISITION, epoch_now(), &serde_json::json!(stopped)) {
            tracing::warn!("journal: {e:#}");
        }
        // Undone while Lidarr was being told, or an open request of the album left by a Delete (V2, `keep_fetching`): put it back
        // at once.
        let kept = self.journal.result(entry.id, super::requests::KEPT_FOR_REQUESTS).ok().flatten().is_some();
        if kept || self.journal.get(entry.id).ok().flatten().is_some_and(|e| e.undone) {
            self.resume_fetching(entry.id, title).await;
        }
    }

    /// The requests a Keep made done for their askers, delivered at once ((c), AL02.8) and named on its entry, so its Undo opens them
    /// again (OQ-D1, `undeliver_requests`). In the user data the requests are in (the library's, the server's); each catalogue
    /// folder's library as the running server has it (none: `kind_in`'s own rule). The requests' round delivers the others.
    async fn deliver_requests(&self, id: EventId, album_id: &str) {
        let Some(record) = crate::serve::ids::record_number(album_id) else { return };
        let (ud, c, kinds) = (self.lib.playlists_db().clone(), self.lib.catalogue(), super::requests::library_kinds());
        let delivered = match crate::run::spawn_blocking(move || super::requests::deliver_in(&ud, &c, &kinds, Some(record))).await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("the requests a Keep made done were not delivered: {e}");
                return;
            }
        };
        if !delivered.is_empty()
            && let Err(e) = self.journal.done(id, super::requests::DELIVERED_BY_KEEP, epoch_now(), &serde_json::json!(delivered))
        {
            tracing::warn!("the requests a Keep delivered were not kept for its Undo: {e:#}");
        }
    }

    /// The requests a Keep delivered (`deliver_requests`), opened again by its Undo as they were (OQ-D1).
    async fn undeliver_requests(&self, id: EventId) {
        let delivered: Vec<super::requests::Closed> =
            self.journal.result(id, super::requests::DELIVERED_BY_KEEP).ok().flatten().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
        if delivered.is_empty() {
            return;
        }
        let ud = self.lib.playlists_db().clone();
        if let Err(e) = crate::run::spawn_blocking(move || super::requests::undeliver(&ud, &delivered)).await {
            tracing::warn!("the requests a Keep delivered were not opened again: {e}");
        }
    }

    /// The requests a Delete closed (Y4, kept on its entry by `requests::reject`), opened again as they were, in the user data the
    /// requests are in (the library's, the server's).
    async fn reopen_requests(&self, id: EventId) {
        let closed: Vec<super::requests::Closed> =
            self.journal.result(id, super::requests::CLOSED_BY_DELETE).ok().flatten().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
        if closed.is_empty() {
            return;
        }
        let ud = self.lib.playlists_db().clone();
        if let Err(e) = crate::run::spawn_blocking(move || super::requests::reopen(&ud, &closed)).await {
            tracing::warn!("the requests a Delete closed were not opened again: {e}");
        }
    }

    /// Lidarr goes on with what a Delete stopped for its copy, where it left an open request of the album (V2, `requests::deleted`):
    /// said on its entry first, so a handler still telling Lidarr puts it back too (`handle`), then put back. Safe to repeat.
    pub(super) async fn keep_fetching(&self, id: EventId, title: &str) {
        if let Err(e) = self.journal.done(id, super::requests::KEPT_FOR_REQUESTS, epoch_now(), &serde_json::json!(true)) {
            tracing::warn!("journal: {e:#}");
        }
        self.resume_fetching(id, title).await;
    }

    /// Lidarr fetches again what it fetched before, if it was told (or was about to be told) to stop. Safe to repeat.
    async fn resume_fetching(&self, id: EventId, title: &str) {
        let read = |h: &str| -> Option<Vec<i64>> { self.journal.result(id, h).ok().flatten().and_then(|v| serde_json::from_value(v).ok()) };
        let stopped = read(ACQUISITION).or_else(|| read(ACQUISITION_PLAN)).unwrap_or_default();
        if stopped.is_empty() {
            return;
        }
        if let Some(l) = self.acquisition()
            && let Err(e) = l.set_fetching(&stopped, true).await
        {
            tracing::warn!("Lidarr: could not fetch {title} again: {e:#}");
        }
    }

    /// Finish or forget interrupted actions, then handle what the journal shows as not done (a restart, Lidarr down),
    /// back as far as the trash keeps things.
    pub async fn catch_up(&self) {
        self.reconcile().await;
        let since = epoch_now() - i64::from(self.trash_days()) * 86_400;
        let mut entries = Vec::new();
        for handler in [ACQUISITION, PLAYDATA] {
            match self.journal.pending(handler, since) {
                Ok(list) => entries.extend(list),
                Err(e) => tracing::warn!("journal: {e:#}"),
            }
        }
        entries.sort_by_key(|e| e.id);
        entries.dedup_by_key(|e| e.id);
        for e in &entries {
            self.handle(e).await;
        }
    }

    /// Hear of new journal events and react to them, for as long as Cratarr runs.
    pub fn listen(self: &Arc<Self>) {
        let (me, mut rx) = (Arc::downgrade(self), self.journal.subscribe());
        crate::run::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(id) => {
                        let Some(me) = me.upgrade() else { return };
                        if let Ok(Some(entry)) = me.journal.get(id) {
                            me.handle(&entry).await;
                        }
                    }
                    // Fell behind or closed: the background pass catches up.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                }
            }
        });
    }

    /// Empty Incoming trash older than the trash days set in Settings.
    pub async fn purge_trash(&self) {
        let folders = self.lib.catalogue().folders();
        let (now, keep, files) = (epoch_now(), self.trash_days(), self.files.clone());
        let gone = crate::run::spawn_blocking(move || {
            folders.paths().map(|f| files.purge(f, now, keep).unwrap_or_else(|e| {
                tracing::warn!("emptying the trash in {}: {e:#}", f.display());
                0
            })).sum::<usize>()
        })
        .await
        .unwrap_or(0);
        if gone > 0 {
            tracing::info!("emptied {gone} old deletion(s) from the libraries' trash");
        }
        // What the trash no longer holds can no longer be undone.
        if let Err(e) = self.journal.prune(now - (i64::from(keep) + 1) * 86_400) {
            tracing::warn!("journal: {e:#}");
        }
    }

    /// Each connected server with Incoming libraries, and their ids.
    fn incoming_sources(&self) -> Vec<(Arc<Active>, Arc<dyn Source>, Vec<String>)> {
        self.lib
            .connected()
            .into_iter()
            .filter(|(a, _)| !a.incoming().is_empty())
            .map(|(a, s)| {
                let ids = a.incoming();
                (a, s, ids)
            })
            .collect()
    }

    async fn read_incoming(&self, a: &Active, s: &dyn Source, libraries: &[String]) -> IncomingSourceView {
        let ShelfRead { supported, albums, error } = self.shelves.read(a, s, libraries, Shelf::Incoming, INCOMING_LIMIT).await;
        IncomingSourceView {
            id: a.cfg.id.clone(),
            label: label_of(&a.cfg),
            supported,
            albums,
            error,
            as_of: epoch_now(),
            deletable: Vec::new(),
            lacks: BTreeMap::new(),
            libraries: libraries.to_vec(),
        }
    }
}

/// Which side of a move to tell the servers about: the folders files went to, the ones they left, or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Told {
    Both,
    Came,
    Gone,
}

/// Tell each server, in its own paths, which files went and where they appeared: `(gone, came)`. The trash is
/// hidden from the servers, so neither side is told about it.
pub(super) async fn tell_moved(lib: &Library, title: &str, moved: &[((i64, String), (i64, String))], which: Told) {
    let folders = lib.catalogue().folders();
    for (a, s) in lib.connected() {
        let server_path = |(folder, rel): &(i64, String)| -> Option<String> {
            if rel.starts_with(&format!("{}/", crate::trash::TRASH_DIR)) {
                return None;
            }
            let f = folders.get(*folder)?.display().to_string();
            a.placement.root(&f).map(|root| format!("{root}{rel}"))
        };
        // By album folder, not file by file: Jellyfin acted on the folder a Keep left (Incoming) but not on 12 new files
        // in a folder it did not know yet (2026-10-13); told of the new folder itself, it scans it.
        let folder_of = |(folder, rel): &(i64, String)| (*folder, rel.rsplit_once('/').map_or(rel.as_str(), |(dir, _)| dir).to_string());
        let mut changes: Vec<(String, bool)> = Vec::new();
        for (gone, came) in moved {
            for (path, created) in [(server_path(&folder_of(gone)), false), (server_path(&folder_of(came)), true)] {
                if (created && which == Told::Gone) || (!created && which == Told::Came) {
                    continue;
                }
                if let Some(p) = path.filter(|p| !changes.iter().any(|(q, _)| q == p)) {
                    changes.push((p, created));
                }
            }
        }
        if !changes.is_empty()
            && let Err(e) = s.files_changed(&changes).await
        {
            tracing::warn!("{}: could not tell it about {title}: {e:#}", a.cfg.id);
        }
    }
}

/// Why Move and Save to never take an Incoming library as their target (F34): the owner's words.
pub(crate) const INCOMING_IS_NO_DESTINATION: &str = "An Incoming library is filled by its fetcher alone.";

#[cfg(test)]
mod tests {
    use super::*;

    fn album(id: &str, path: &str, format: &str) -> source::Album {
        source::Album {
            id: id.into(),
            title: "Zebra".into(),
            artist: Some("Arp".into()),
            path: Some(path.into()),
            tracks: 9,
            format: Some(format.into()),
            bitrate_kbps: Some(320),
            ..Default::default()
        }
    }

    /// The service on a main library `music` (catalogue folder 0) and an Incoming `inc` (1), no server, on the disk.
    pub(super) async fn on_disk(music: &std::path::Path, inc: &std::path::Path) -> IncomingService {
        std::fs::create_dir_all(music.join(crate::playlists::PLAYLIST_DIR)).unwrap();
        let db = Arc::new(crate::userdata::UserData::open(None).unwrap());
        let lib = Library::new(None, Some(music.to_string_lossy().into_owned()), Vec::new(), Default::default(), None, db, None).await.unwrap();
        lib.catalogue().set_folders(vec![music.to_path_buf(), inc.to_path_buf()]).unwrap();
        // Music's layout known, as Settings' default (PL1): these tests place by it; PL1's own take it away (`layout_tests`).
        lib.set_saved_template(Some(crate::naming::Template::default()));
        let shelves = Arc::new(Shelves::new(lib.clone(), None));
        IncomingService::new(lib, crate::ports::disk(), Arc::new(Journal::open(None).unwrap()), None, crate::trash::DEFAULT_KEEP_DAYS, None, shelves)
    }

    pub(super) fn folder(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cratarr-incoming-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// V2 (the review of run 8b, finding 2): a Delete that left another asker's request of the album has Lidarr go on: Incoming's
    /// handler puts back what it stopped, whether it was told so before it stopped Lidarr or after.
    #[tokio::test]
    async fn a_delete_that_left_a_request_has_lidarr_go_on() {
        use axum::Json;
        use axum::routing::{get, put};
        let told: Arc<std::sync::Mutex<Vec<bool>>> = Arc::default();
        let t = told.clone();
        let app = axum::Router::new().route("/api/v1/album", get(|| async { Json(serde_json::json!([{ "id": 7, "monitored": true }])) })).route(
            "/api/v1/album/monitor",
            put(move |Json(b): Json<serde_json::Value>| {
                t.lock().unwrap().push(b["monitored"] == true);
                async { Json(serde_json::json!({})) }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        crate::run::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let (music, inc) = (folder("kept-music"), folder("kept-inc"));
        let svc = on_disk(&music, &inc).await;
        svc.apply_settings(lidarr_pair(Some(settings::LidarrConfig { url, api_key: "key".into(), enabled: true, ..Default::default() })), crate::trash::DEFAULT_KEEP_DAYS);
        for before in [true, false] {
            let release_group = Some("00000000-0000-4000-8000-000000000340".to_string());
            let event = Event::AlbumRejected { source: "cratarr".into(), album_id: "al-1".into(), title: "Album".into(), folder: 1, dir: "A/Album".into(), token: "t".into(), release_group };
            let id = svc.journal.intend(epoch_now(), &event).unwrap();
            svc.journal.applied(id).unwrap();
            if before {
                svc.keep_fetching(id, "Album").await;
            }
            svc.handle(&svc.journal.get(id).unwrap().unwrap()).await;
            if !before {
                svc.keep_fetching(id, "Album").await;
            }
            assert_eq!(told.lock().unwrap().last(), Some(&true), "told before it acts {before}: Lidarr fetches it again");
        }
        for d in [music, inc] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// A Keep of `X/A`'s two tracks from Incoming (1) into `Y` in the library (0).
    fn keep_of_two() -> Event {
        let mv = |n: &str| TrackMove { track: None, from: (1, format!("X/A/{n}")), to: (0, format!("Y/{n}")), was: None };
        Event::AlbumAdopted {
            source: "cratarr".into(),
            album_id: "al-1".into(),
            title: "A".into(),
            from_dir: (1, "X/A".into()),
            to_dir: (0, "Y".into()),
            moves: vec![mv("1.flac"), mv("2.flac")],
            release_group: None,
            converted: Vec::new(),
        }
    }

    fn two_tracks(inc: &std::path::Path) {
        std::fs::create_dir_all(inc.join("X/A")).unwrap();
        std::fs::write(inc.join("X/A/1.flac"), b"one").unwrap();
        std::fs::write(inc.join("X/A/2.flac"), b"two").unwrap();
    }

    /// A pass does not stop for an action, so what it reads can be a copy half way (`Catalogue::copies_moving`).
    #[tokio::test]
    async fn an_action_under_way_holds_the_copies_on_their_records_to_the_end_of_its_move() {
        let (music, inc) = (folder("holding-music"), folder("holding-inc"));
        let svc = on_disk(&music, &inc).await;
        two_tracks(&inc);
        let underway = svc.intend(100, &keep_of_two()).unwrap();
        assert!(svc.lib.catalogue().copies_are_moving());
        // The request dropped, its move still going on the blocking pool.
        let held = underway.clone();
        drop(underway);
        assert!(svc.lib.catalogue().copies_are_moving(), "held for as long as the files are moving");
        drop(held);
        assert!(!svc.lib.catalogue().copies_are_moving());
        for d in [music, inc] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    #[tokio::test]
    async fn a_keep_under_way_survives_the_pass_that_finishes_crashed_ones() {
        let (music, inc) = (folder("underway-music"), folder("underway-inc"));
        let svc = on_disk(&music, &inc).await;
        two_tracks(&inc);
        let underway = svc.intend(100, &keep_of_two()).unwrap();
        let id = underway.id();

        svc.catch_up().await;
        assert!(svc.journal.get(id).unwrap().is_some_and(|e| !e.applied), "its first file has not arrived: no crash, its event stays");
        // The request dropped, its move still going on the blocking pool.
        let held = underway.clone();
        drop(underway);
        svc.catch_up().await;
        assert!(svc.journal.get(id).unwrap().is_some(), "held for as long as the files are moving");
        // Nothing holds it now: what the recurring pass is for.
        drop(held);
        svc.catch_up().await;
        assert!(svc.journal.get(id).unwrap().is_none(), "nothing arrived: it never happened");
        for d in [music, inc] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    #[tokio::test]
    async fn a_keep_whose_request_dropped_is_finished_by_the_pass() {
        let (music, inc) = (folder("dropped-music"), folder("dropped-inc"));
        let svc = on_disk(&music, &inc).await;
        two_tracks(&inc);
        let underway = svc.intend(100, &keep_of_two()).unwrap();
        let id = underway.id();
        std::fs::create_dir_all(music.join("Y")).unwrap();
        std::fs::rename(inc.join("X/A/1.flac"), music.join("Y/1.flac")).unwrap();
        drop(underway);

        svc.catch_up().await;
        assert!(svc.journal.get(id).unwrap().is_some_and(|e| e.applied));
        assert_eq!(std::fs::read(music.join("Y/2.flac")).unwrap(), b"two");
        for d in [music, inc] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    #[tokio::test]
    async fn a_keep_stopped_on_its_first_copy_is_forgotten_and_nothing_stands_in_the_way_of_trying_again() {
        let (music, inc) = (folder("first-music"), folder("first-inc"));
        let svc = on_disk(&music, &inc).await;
        two_tracks(&inc);
        let id = svc.journal.intend(100, &keep_of_two()).unwrap();
        std::fs::create_dir_all(music.join("Y")).unwrap();
        std::fs::write(crate::moves::part_of(&music.join("Y/1.flac")), b"on").unwrap();

        svc.reconcile().await;
        assert!(svc.journal.get(id).unwrap().is_none(), "it never happened");
        assert!(!music.join("Y").exists(), "plan_keep refuses a folder that is there");
        assert_eq!(std::fs::read(inc.join("X/A/1.flac")).unwrap(), b"one");
        for d in [music, inc] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    #[tokio::test]
    async fn a_keep_stopped_on_a_later_copy_is_finished_whole() {
        let (music, inc) = (folder("later-music"), folder("later-inc"));
        let svc = on_disk(&music, &inc).await;
        two_tracks(&inc);
        let id = svc.journal.intend(100, &keep_of_two()).unwrap();
        std::fs::create_dir_all(music.join("Y")).unwrap();
        std::fs::rename(inc.join("X/A/1.flac"), music.join("Y/1.flac")).unwrap();
        std::fs::write(crate::moves::part_of(&music.join("Y/2.flac")), b"t").unwrap();

        svc.reconcile().await;
        assert!(svc.journal.get(id).unwrap().is_some_and(|e| e.applied), "the album's Undo stands");
        assert_eq!(std::fs::read(music.join("Y/2.flac")).unwrap(), b"two");
        assert!(!inc.join("X").exists(), "the album is not split between two libraries");
        for d in [music, inc] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// The same release fetched into Incoming again, a track changed by its covers being completed and the other alike to the
    /// byte: what stands at Undo's targets refuses it, and a refusal touches nothing.
    #[tokio::test]
    async fn an_undo_refused_by_the_same_release_fetched_again_leaves_the_album_whole() {
        let (music, inc) = (folder("again-music"), folder("again-inc"));
        let svc = on_disk(&music, &inc).await;
        two_tracks(&inc);
        let event = keep_of_two();
        let id = svc.journal.intend(100, &event).unwrap();
        crate::moves::apply(&svc.steps(&event).unwrap()).unwrap();
        svc.journal.applied(id).unwrap();
        two_tracks(&inc);
        std::fs::write(inc.join("X/A/1.flac"), b"one with its cover inside").unwrap();

        let refused = svc.undo(&id.to_string()).await.unwrap_err();
        assert!(matches!(&refused, ToggleError::Refused(why) if why.contains("exists already")), "{refused:?}");
        assert_eq!(std::fs::read(music.join("Y/1.flac")).unwrap(), b"one");
        assert_eq!(std::fs::read(music.join("Y/2.flac")).unwrap(), b"two", "the library's album keeps every file");
        assert_eq!(std::fs::read(inc.join("X/A/1.flac")).unwrap(), b"one with its cover inside");
        assert!(svc.journal.get(id).unwrap().is_some_and(|e| !e.undone));
        for d in [music, inc] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// A library's list entry for its copy of `id`, lying at `dir` in catalogue folder `folder`.
    pub(super) fn copy_at(id: &str, folder: i64, library: &str, dir: &str) -> source::Album {
        let copy = source::AlbumCopy { library: library.into(), folder, dir: dir.into(), ..Default::default() };
        source::Album { library: Some(library.into()), copies: vec![copy], ..album(id, &format!("{library}/{dir}"), "FLAC") }
    }

    /// Two copies of one record, one folder name in each library: the case where the path's end cannot tell them apart.
    fn in_both(music: &std::path::Path, inc: &std::path::Path) {
        for (root, body) in [(music, b"kept".as_slice()), (inc, b"waiting".as_slice())] {
            std::fs::create_dir_all(root.join("A/Album")).unwrap();
            std::fs::write(root.join("A/Album/1.flac"), body).unwrap();
        }
    }

    /// One folder name in Music, Incoming and two accounts' own music, the catalogue knowing all four: by the path's end one
    /// account's Delete lands in another's own music, and an administrator's in Incoming. What is named goes, and nothing else,
    /// whichever it is, and its Undo brings that one back.
    #[tokio::test]
    async fn delete_names_the_copy() {
        let roots = [folder("named-music"), folder("named-inc"), folder("named-ann"), folder("named-bob")];
        let svc = on_disk(&roots[0], &roots[1]).await;
        let c = svc.lib.catalogue();
        c.set_folders(roots.to_vec()).unwrap();
        for (n, root) in roots.iter().enumerate() {
            std::fs::create_dir_all(root.join("A/Album")).unwrap();
            std::fs::write(root.join("A/Album/1.flac"), [n as u8]).unwrap();
        }
        c.walk_all();
        c.pretend_read_for_tests();
        c.identify().unwrap();
        let standing = |gone: Option<usize>| roots.iter().enumerate().filter(|(n, _)| Some(*n) != gone).all(|(n, r)| std::fs::read(r.join("A/Album/1.flac")).is_ok_and(|b| b == [n as u8]));
        for (n, root) in roots.iter().enumerate() {
            let named = svc.delete_listed("cratarr", copy_at("al-1", n as i64, &root.to_string_lossy(), "A/Album")).await.unwrap();
            assert!(!root.join("A/Album").exists(), "the copy named went: {n}");
            assert!(standing(Some(n)), "every other copy stands: {n}");
            assert_eq!(crate::trash::holds(root, &format!("{}/A/Album", named.undo)), false, "the key is the journal's, not the trash's");
            svc.undo(&named.undo).await.unwrap();
            assert!(standing(None), "back where it was: {n}");
        }
        for d in roots {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// G8: an administrator is sent every job; a user only those they queued; nobody signed in none.
    #[test]
    fn a_user_is_sent_their_own_jobs_alone() {
        let job = |id: u64, by: Option<&str>| JobView { by: by.map(str::to_string), ..JobView::converting(id, "cratarr", "al-1", "/in", 1, "A/Album") };
        let jobs = vec![job(1, Some("ann")), job(2, Some("bob")), job(3, None)];
        let ids = |v: Vec<JobView>| v.into_iter().map(|j| j.id).collect::<Vec<_>>();
        assert_eq!(ids(shown_jobs(jobs.clone(), Some("root"), true)), [1, 2, 3], "an administrator: every one");
        assert_eq!(ids(shown_jobs(jobs.clone(), Some("ann"), false)), [1], "ann: her own");
        assert_eq!(ids(shown_jobs(jobs, None, false)), Vec::<u64>::new(), "nobody: none");
    }

    /// U3 (the review of 8c/8d, finding 3): a Delete through Incoming's own endpoint (an administrator's, before the next Soulseek
    /// round reads the folder) forgets the record's Placed Soulseek items, as App's Delete does: one left would keep the record's
    /// next request from Soulseek for good. Other records' items stay, and a Delete in a main library forgets nothing.
    #[tokio::test]
    async fn an_admin_delete_before_the_round_still_frees_the_record() {
        let (music, inc) = (folder("u3-music"), folder("u3-inc"));
        let svc = on_disk(&music, &inc).await;
        in_both(&music, &inc);
        let c = svc.lib.catalogue();
        // A record of its own: the other tests here delete record 1's copy, which forgets record 1's Placed items.
        c.number_above(9_300).unwrap();
        c.walk_all();
        c.pretend_read_for_tests();
        c.identify().unwrap();
        crate::app::soulseek::placed_for_tests("ss-u3-placed", 9_301);
        crate::app::soulseek::placed_for_tests("ss-u3-other", 9_302);
        svc.delete_listed("cratarr", copy_at("al-9301", 0, &music.to_string_lossy(), "A/Album")).await.unwrap();
        assert!(crate::app::soulseek::listed_for_tests("ss-u3-placed"), "a Delete in Music forgets nothing");
        svc.delete_listed("cratarr", copy_at("al-9301", 1, &inc.to_string_lossy(), "A/Album")).await.unwrap();
        assert!(!crate::app::soulseek::listed_for_tests("ss-u3-placed"), "forgotten");
        assert!(crate::app::soulseek::listed_for_tests("ss-u3-other"), "another record's stays");
        crate::app::soulseek::forget_placed(9_302);
        for d in [music, inc] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// Each file under `dir` (hidden ones too), by its path there, with its bytes and its time.
    pub(super) fn lying(dir: &std::path::Path) -> std::collections::BTreeMap<String, (Vec<u8>, std::time::SystemTime)> {
        let mut out = std::collections::BTreeMap::new();
        let mut todo = vec![dir.to_path_buf()];
        while let Some(d) = todo.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    todo.push(p);
                } else {
                    let rel = p.strip_prefix(dir).unwrap().to_string_lossy().replace('\\', "/");
                    out.insert(rel, (std::fs::read(&p).unwrap(), std::fs::metadata(&p).unwrap().modified().unwrap()));
                }
            }
        }
        out
    }

    /// Music's album (catalogue folder 0, record 1), Incoming (1) and someone's own music (2): the service, and the three folders.
    async fn with_own_music(tag: &str) -> (IncomingService, PathBuf, PathBuf, PathBuf) {
        let (music, inc, own) = (folder(&format!("{tag}-music")), folder(&format!("{tag}-inc")), folder(&format!("{tag}-own")));
        let svc = on_disk(&music, &inc).await;
        let c = svc.lib.catalogue();
        c.set_folders(vec![music.clone(), inc.clone(), own.clone()]).unwrap();
        std::fs::create_dir_all(music.join("A/Album")).unwrap();
        for (n, body) in [("1.flac", b"one".as_slice()), ("2.flac", b"two".as_slice())] {
            std::fs::write(music.join("A/Album").join(n), body).unwrap();
        }
        c.walk_all();
        c.pretend_read_for_tests();
        c.identify().unwrap();
        (svc, music, inc, own)
    }

    /// C40: Copy to copies Music's album into someone's own music and leaves it where it was: the copy holds exactly the
    /// original's files, their bytes and times, and nothing else is written, beside the copy or into the original (the owner: only
    /// what the sheet shows); its files are on the same tracks, one album with two copies; the journal holds one Copy to.
    #[tokio::test]
    async fn copy_to_own_music_leaves_the_record_where_it_was() {
        let (svc, music, inc, own) = with_own_music("c40-copy").await;
        let before = lying(&music);
        let at = svc.locate_listed("cratarr", copy_at("al-1", 0, &music.to_string_lossy(), "A/Album")).await.unwrap();
        const ACCOUNT: &str = "c40-copier";
        let who = crate::serve::who::Who { user: ACCOUNT.into(), admin: true, scope: crate::serve::who::Scope::Full, home: true };
        let done = crate::serve::who::acting_as(who, svc.copying(&at, 2, None, None)).await.unwrap();
        let tasks = crate::runner::runner().of(ACCOUNT);
        assert!(tasks.iter().any(|t| t.kind == "copy" && t.finished.is_some() && t.error.is_none()), "its progress a task of its account's: {} tasks", tasks.len());
        assert_eq!((done.folder.as_str(), done.queued), ("A/Album", false));
        assert_eq!(lying(&music), before, "the original as it was, nothing written into or beside it");
        let copied: std::collections::BTreeMap<String, (Vec<u8>, std::time::SystemTime)> = lying(&own);
        let expected: std::collections::BTreeMap<String, (Vec<u8>, std::time::SystemTime)> = before.into_iter().collect();
        assert_eq!(copied, expected, "the copy is the original's files alone, their bytes and their times");
        let c = svc.lib.catalogue();
        for f in ["1.flac", "2.flac"] {
            let (original, copy) = (c.track_of(0, &format!("A/Album/{f}")), c.track_of(2, &format!("A/Album/{f}")));
            assert!(original.is_some() && original == copy, "{f}: on the same track");
        }
        let entry = svc.journal.get(done.undo.unwrap().parse().unwrap()).unwrap().unwrap();
        assert!(entry.applied && matches!(entry.event, Event::AlbumCopied { .. }));
        for d in [music, inc, own] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// C40: a Copy to's Undo takes the copy into its library's trash, its rows with it, and leaves the original as it was; once.
    #[tokio::test]
    async fn undo_takes_the_copy_away() {
        let (svc, music, inc, own) = with_own_music("c40-undo").await;
        let before = lying(&music);
        let at = svc.locate_listed("cratarr", copy_at("al-1", 0, &music.to_string_lossy(), "A/Album")).await.unwrap();
        let undo = svc.copying(&at, 2, None, None).await.unwrap().undo.unwrap();
        assert_eq!(svc.undo(&undo).await.unwrap(), "Zebra", "its title");
        assert!(!own.join("A/Album").exists(), "the copy gone");
        assert_eq!(lying(&own.join(crate::trash::TRASH_DIR)).len(), 2, "into its library's trash");
        assert_eq!(lying(&music), before, "the original as it was");
        let c = svc.lib.catalogue();
        assert!(!c.has_row(2, "A/Album/1.flac") && c.has_row(0, "A/Album/1.flac"), "the copy's rows into the trash too");
        assert!(svc.undo(&undo).await.is_err(), "undone once");
        for d in [music, inc, own] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// RF10 (the review of runs 11 and 12, RA10; the owner's "writes nothing the sheet doesn't show"): Copy to's tap carries what its
    /// sheet showed, the copy's folder where it goes, how many files and the copy's own folder; it copies as shown. A tap that differs
    /// is refused, with nothing written: another folder, another copy, another count, or a file come into Music's folder since (a
    /// booklet: 3 files, not the sheet's 2).
    #[tokio::test]
    async fn a_copy_tap_carries_its_sheet_and_one_that_differs_is_refused() {
        let (svc, music, inc, own) = with_own_music("rf10-sheet").await;
        let at = svc.locate_listed("cratarr", copy_at("al-1", 0, &music.to_string_lossy(), "A/Album")).await.unwrap();
        let shown = CopySheet { folder: "A/Album".into(), files: 2, from_dir: "A/Album".into() };
        let differing = [
            (CopySheet { folder: "A/Other".into(), ..shown.clone() }, "another folder"),
            (CopySheet { from_dir: "B/Album".into(), ..shown.clone() }, "another copy"),
            (CopySheet { files: 3, ..shown.clone() }, "another count"),
        ];
        for (other, what) in differing {
            let e = svc.copying(&at, 2, None, Some(&other)).await.expect_err(what);
            assert!(e.to_string().contains("Changed since you looked"), "{what}: {e}");
            assert!(!own.join("A").exists(), "{what}: nothing written");
        }
        std::fs::write(music.join("A/Album/booklet.pdf"), b"come since").unwrap();
        let e = svc.copying(&at, 2, None, Some(&shown)).await.expect_err("a file come since");
        assert!(e.to_string().contains("Changed since you looked"), "{e}");
        assert!(!own.join("A").exists(), "nothing written");
        std::fs::remove_file(music.join("A/Album/booklet.pdf")).unwrap();
        let done = svc.copying(&at, 2, None, Some(&shown)).await.unwrap();
        assert_eq!(done.folder, "A/Album");
        assert_eq!(lying(&own.join("A/Album")).len(), 2, "as its sheet showed");
        for d in [music, inc, own] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// RY01's titles, a track's in its number's place.
    const TITLES: [&str; 10] = ["One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten"];

    /// RY01's folder, as Music's template names the record's: where its copies go.
    pub(super) const RY01_DIR: &str = "A - Album (2019)";

    /// A file of RY01's shape in `dir`, as the default template names it: `01 A - One.flac`.
    pub(super) fn ry01_file(dir: &str, n: u32, ext: &str) -> String {
        let name = format!("{n:02} A - {}.{ext}", TITLES[n as usize - 1]);
        if dir.is_empty() { name } else { format!("{dir}/{name}") }
    }

    /// RY01's shape (SCENARIOS-RUN-12's `with_three_own`): Music (catalogue folder 0, the main library), Incoming (1), Ann's own music
    /// (2) and Bob's (3). Music's copy at `music_dir`, `tracks` MP3 and a cover.jpg; Bob's at `bob_dir`, as many FLAC and a cover.jpg
    /// of his own; each file tagged as Music's template names it (the year where one is given, `of` the total each says), Bob's on
    /// Music's tracks, as copies of one record are. The service, and the four folders.
    pub(super) async fn with_three_own(tag: &str, music_dir: &str, bob_dir: &str, year: Option<&str>, tracks: u32, of: u32) -> (IncomingService, PathBuf, PathBuf, PathBuf, PathBuf) {
        let roots: Vec<PathBuf> = ["music", "inc", "ann", "bob"].iter().map(|n| folder(&format!("{tag}-{n}"))).collect();
        let svc = on_disk(&roots[0], &roots[1]).await;
        let c = svc.lib.catalogue();
        c.set_folders(roots.clone()).unwrap();
        for (root, dir, ext) in [(&roots[0], music_dir, "mp3"), (&roots[3], bob_dir, "flac")] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
            for n in 1..=tracks {
                std::fs::write(root.join(ry01_file(dir, n, ext)), format!("{ext} number {n}")).unwrap();
            }
            std::fs::write(root.join(dir).join("cover.jpg"), format!("{ext} cover")).unwrap();
        }
        c.walk_all();
        c.pretend_read_for_tests();
        for (f, dir, ext) in [(0, music_dir, "mp3"), (3, bob_dir, "flac")] {
            for n in 1..=tracks {
                let (number, total) = (n.to_string(), of.to_string());
                let mut tags = vec![("albumartist", "A"), ("artist", "A"), ("album", "Album"), ("title", TITLES[n as usize - 1]), ("tracknumber", number.as_str()), ("totaltracks", total.as_str())];
                if let Some(y) = year {
                    tags.push(("date", y));
                }
                c.tag_for_tests(f, &ry01_file(dir, n, ext), &tags);
            }
        }
        c.identify().unwrap();
        for n in 1..=tracks {
            let track = c.track_of(0, &ry01_file(music_dir, n, "mp3")).unwrap();
            c.give_track_for_tests(3, &ry01_file(bob_dir, n, "flac"), track);
        }
        let mut roots = roots.into_iter();
        let mut next = || roots.next().unwrap();
        (svc, next(), next(), next(), next())
    }

    /// Bob's copy at `dir` as RY01 finds it listed (catalogue folder 3).
    pub(super) async fn bobs_copy(svc: &IncomingService, bob: &std::path::Path, dir: &str) -> Located {
        svc.locate_listed("cratarr", copy_at("al-1", 3, &bob.to_string_lossy(), dir)).await.unwrap()
    }

    /// A row's track and trash mark, present or not: (track, trashed); none where there is no row.
    pub(super) fn row_at(c: &crate::catalogue::Catalogue, folder: i64, rel: &str) -> Option<(Option<i64>, Option<String>)> {
        use rusqlite::OptionalExtension;
        let db = c.writer_for_tests();
        db.query_row("SELECT track, trashed FROM files WHERE folder = ?1 AND rel = ?2", rusqlite::params![folder, rel], |r| Ok((r.get(0)?, r.get(1)?))).optional().unwrap()
    }

    /// What the log said, for a test that reads it.
    #[derive(Clone, Default)]
    struct Logged(Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Logged {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Logged {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    /// RY01.7: a Replace from Bob's copy stopped at any point (its intent in the journal, nothing holding it) is finished or put back
    /// by the pass. (1) Nothing moved, a part of the first copy beside Music's files: forgotten, the part gone, Music as it was. (2)
    /// Music's old files in its trash and the first copy made: finished forward, every copy byte for byte with Bob's time, on the old
    /// files' tracks, the old rows trashed holding none, applied and undoable. (2') The same, Bob's second file gone since: put back,
    /// not half-finished: the copy made taken away, Music's old files back on their tracks, the entry forgotten, a warning logged.
    /// (3) Every file in place, the rows not followed: they follow, applied. Never an entry applied while Music holds part of a copy;
    /// Music's album never left in the trash with no copy in its place; Bob's files never moved.
    #[tokio::test]
    async fn a_replace_from_a_copy_stopped_at_any_point_is_finished_or_put_back() {
        for case in ["1", "2", "2'", "3"] {
            let (svc, music, inc, ann, bob) = with_three_own(&format!("ry01-stop{}", case.replace('\'', "b")), RY01_DIR, RY01_DIR, Some("2019"), 2, 2).await;
            let (before, mut theirs) = (lying(&music), lying(&bob));
            let at = bobs_copy(&svc, &bob, RY01_DIR).await;
            let (event, ..) = svc.event_from_copy(&at, (0, RY01_DIR.into()), None, epoch_now()).await.unwrap();
            let id = svc.journal.intend(100, &event).unwrap();
            let Event::AlbumReplacedFromCopy { replaced, .. } = &event else { unreachable!() };
            let mut out = svc.steps(&event).unwrap();
            let ins = out.split_off(replaced.len());
            let c = svc.lib.catalogue();
            let olds: Vec<Option<i64>> = (1..=2).map(|n| c.track_of(0, &ry01_file(RY01_DIR, n, "mp3"))).collect();
            assert!(olds.iter().all(Option::is_some));
            match case {
                "1" => std::fs::write(crate::moves::part_of(&ins[0].to.0.join(&ins[0].to.1)), b"part").unwrap(),
                "3" => {
                    crate::moves::apply(&out).unwrap();
                    crate::moves::copy(&ins).unwrap();
                }
                _ => {
                    crate::moves::apply(&out).unwrap();
                    crate::moves::clone_file(&ins[0].from.0.join(&ins[0].from.1), &ins[0].to.0.join(&ins[0].to.1)).unwrap();
                    if case == "2'" {
                        let gone = ry01_file(RY01_DIR, 2, "flac");
                        std::fs::remove_file(bob.join(&gone)).unwrap();
                        theirs.remove(&gone);
                    }
                }
            }
            let log = Logged::default();
            let writer = log.clone();
            let subscriber = tracing_subscriber::fmt().with_writer(move || writer.clone()).with_ansi(false).with_max_level(tracing::Level::INFO).finish();
            let logging = tracing::subscriber::set_default(subscriber);
            svc.reconcile().await;
            drop(logging);
            let entry = svc.journal.get(id).unwrap();
            if matches!(case, "1" | "2'") {
                assert!(entry.is_none(), "{case}: forgotten");
                assert_eq!(lying(&music), before, "{case}: Music's album as it was, nothing of the copy, no part");
                for (n, t) in (1..=2).zip(&olds) {
                    assert_eq!(c.track_of(0, &ry01_file(RY01_DIR, n, "mp3")), *t, "{case}: its rows on their tracks");
                }
                assert!(!c.has_row(0, &ry01_file(RY01_DIR, 1, "flac")), "{case}: no row of a copy");
            } else {
                assert!(entry.is_some_and(|e| e.applied), "{case}: applied");
                for n in 1..=2 {
                    let (copy, original) = (music.join(ry01_file(RY01_DIR, n, "flac")), bob.join(ry01_file(RY01_DIR, n, "flac")));
                    assert_eq!(std::fs::read(&copy).unwrap(), std::fs::read(&original).unwrap(), "{case}: byte for byte");
                    assert_eq!(std::fs::metadata(&copy).unwrap().modified().unwrap(), std::fs::metadata(&original).unwrap().modified().unwrap(), "{case}: with Bob's time");
                    assert_eq!(c.track_of(0, &ry01_file(RY01_DIR, n, "flac")), olds[n as usize - 1], "{case}: on the old file's track");
                    let trashed = replaced.iter().find(|r| r.from.1 == ry01_file(RY01_DIR, n, "mp3")).unwrap();
                    let row = row_at(&c, 0, &trashed.to.1).expect("the old row in the trash");
                    assert!(row.0.is_none() && row.1.is_some(), "{case}: the old row trashed, holding no track: {row:?}");
                }
                svc.undo(&id.to_string()).await.unwrap();
                let back: std::collections::BTreeMap<String, (Vec<u8>, std::time::SystemTime)> = before.iter().map(|(k, v)| (k.strip_prefix(&format!("{RY01_DIR}/")).unwrap().to_string(), v.clone())).collect();
                assert_eq!(lying(&music.join(RY01_DIR)), back, "{case}: its Undo puts Music's copy back");
            }
            if case == "2'" {
                let text = log.text();
                assert!(text.lines().any(|l| l.contains("WARN") && l.contains("put Zebra back as it was")), "{text}");
            }
            assert_eq!(lying(&bob), theirs, "{case}: Bob's files never moved");
            for d in [music, inc, ann, bob] {
                std::fs::remove_dir_all(d).ok();
            }
        }
    }

    /// A Replace from Bob's copy (RY01) stopped after Music's old files went into its trash and every copy but Bob's second FLAC was
    /// made, `cover.jpg` among them at Music's old cover's name (RF6's shape, RA2's crash): its intent in the journal, nothing holding
    /// it. The service, its folders, the entry, Music as it was before, and the event.
    async fn ry01_stopped_before_its_last_copy(tag: &str) -> (IncomingService, [PathBuf; 4], EventId, std::collections::BTreeMap<String, (Vec<u8>, std::time::SystemTime)>, Event) {
        let (svc, music, inc, ann, bob) = with_three_own(tag, RY01_DIR, RY01_DIR, Some("2019"), 2, 2).await;
        let before = lying(&music);
        let at = bobs_copy(&svc, &bob, RY01_DIR).await;
        let (event, ..) = svc.event_from_copy(&at, (0, RY01_DIR.into()), None, epoch_now()).await.unwrap();
        let id = svc.journal.intend(100, &event).unwrap();
        let Event::AlbumReplacedFromCopy { replaced, .. } = &event else { unreachable!() };
        let mut out = svc.steps(&event).unwrap();
        let ins = out.split_off(replaced.len());
        crate::moves::apply(&out).unwrap();
        for s in ins.iter().filter(|s| !s.from.1.ends_with(&ry01_file("", 2, "flac"))) {
            crate::moves::clone_file(&s.from.0.join(&s.from.1), &s.to.0.join(&s.to.1)).unwrap();
        }
        assert!(music.join(RY01_DIR).join("cover.jpg").is_file() && music.join(ry01_file(RY01_DIR, 1, "flac")).is_file(), "the cover and the first FLAC copied");
        (svc, [music, inc, ann, bob], id, before, event)
    }

    /// The log of what `f` says at INFO and above, on this thread.
    async fn logged<F: std::future::Future<Output = ()>>(f: F) -> String {
        let log = Logged::default();
        let writer = log.clone();
        let subscriber = tracing_subscriber::fmt().with_writer(move || writer.clone()).with_ansi(false).with_max_level(tracing::Level::INFO).finish();
        let logging = tracing::subscriber::set_default(subscriber);
        f.await;
        drop(logging);
        log.text()
    }

    /// RF6 (the review of runs 11 and 12, RA2): a Replace from Bob's copy cut short, then Bob's copy changes before the pass: (1) Bob
    /// deletes his copy (his Delete: into his trash); (2) Bob retags the FLAC already copied (other bytes, other time). The pass cannot
    /// make the copy whole, and puts it back: every copy it made taken away, told by Bob's files as they were when the intent was
    /// written, whatever they are now, `cover.jpg` at Music's old cover's name too; Music's old files back under their names, bytes and
    /// times, on their tracks; no row of a copy; the entry forgotten, a warning logged. Bob's files as he left them. Never a copy of
    /// Bob's left in Music, nor Music's album left in its trash with no entry.
    #[tokio::test]
    async fn a_replace_from_a_copy_put_back_takes_away_what_it_copied_whatever_bobs_copy_became() {
        for case in ["deleted", "retagged"] {
            let (svc, [music, inc, ann, bob], id, before, _) = ry01_stopped_before_its_last_copy(&format!("rf6-{case}")).await;
            let c = svc.lib.catalogue();
            let olds: Vec<Option<i64>> = (1..=2).map(|n| c.track_of(0, &ry01_file(RY01_DIR, n, "mp3"))).collect();
            if case == "deleted" {
                crate::trash::trash(&bob, RY01_DIR, epoch_now()).unwrap();
            } else {
                std::fs::write(bob.join(ry01_file(RY01_DIR, 1, "flac")), b"flac number 1, its tag written since").unwrap();
            }
            let theirs = lying(&bob);
            let text = logged(svc.reconcile()).await;
            assert!(svc.journal.get(id).unwrap().is_none(), "{case}: forgotten");
            assert_eq!(lying(&music), before, "{case}: Music's album back as it was, bytes and times; nothing of Bob's copy left");
            for (n, t) in (1..=2).zip(&olds) {
                assert_eq!(row_at(&c, 0, &ry01_file(RY01_DIR, n, "mp3")), Some((*t, None)), "{case}: on its track, not trashed");
                assert!(!c.has_row(0, &ry01_file(RY01_DIR, n, "flac")), "{case}: no row of a copy");
            }
            assert!(text.lines().any(|l| l.contains("WARN") && l.contains("put Zebra back as it was")), "{case}: {text}");
            assert_eq!(lying(&bob), theirs, "{case}: Bob's files as he left them");
            for d in [music, inc, ann, bob] {
                std::fs::remove_dir_all(d).ok();
            }
        }
    }

    /// RF6: a Replace from Bob's copy cut short, Bob's copy deleted since, and Music's copied `cover.jpg` rewritten since it was copied
    /// (a picture written into it: no copy as made any more), so Music's old cover cannot come back to its name. The pass takes away
    /// the copy it made and leaves the rewritten cover, logs an error, and keeps the intent for the next pass, Music's old files still
    /// in its trash under the entry's token. Once the cover is moved aside, the next pass puts Music's album back and forgets the entry.
    /// Never Music's album left in its trash with no entry; never a file it did not copy taken away.
    #[tokio::test]
    async fn a_replace_from_a_copy_that_cannot_be_put_back_stays_for_the_next_pass() {
        let (svc, [music, inc, ann, bob], id, before, event) = ry01_stopped_before_its_last_copy("rf6-stuck").await;
        let Event::AlbumReplacedFromCopy { replaced, .. } = &event else { unreachable!() };
        crate::trash::trash(&bob, RY01_DIR, epoch_now()).unwrap();
        let cover = music.join(RY01_DIR).join("cover.jpg");
        std::fs::write(&cover, b"a picture written since").unwrap();
        let text = logged(svc.reconcile()).await;
        assert!(svc.journal.get(id).unwrap().is_some_and(|e| !e.applied), "the intent kept for the next pass");
        assert!(text.lines().any(|l| l.contains("ERROR") && l.contains("stays for the next pass")), "an error logged: {text}");
        for r in replaced {
            let (_, (bytes, _)) = before.iter().find(|(rel, _)| **rel == r.from.1).unwrap();
            assert_eq!(std::fs::read(music.join(&r.to.1)).ok().as_ref(), Some(bytes), "{}: still in Music's trash under the entry's token", r.from.1);
        }
        assert!(!music.join(ry01_file(RY01_DIR, 1, "flac")).exists(), "the copy it made taken away");
        assert_eq!(std::fs::read(&cover).unwrap(), b"a picture written since", "the rewritten cover left as it is");
        std::fs::remove_file(&cover).unwrap();
        let text = logged(svc.reconcile()).await;
        assert!(svc.journal.get(id).unwrap().is_none(), "then forgotten");
        assert_eq!(lying(&music), before, "Music's album back as it was");
        assert!(text.lines().any(|l| l.contains("WARN") && l.contains("put Zebra back as it was")), "{text}");
        for d in [music, inc, ann, bob] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// RY01.8: the Undo of a Replace from Bob's copy. (1) Each copied file still as copied goes into Music's trash, file by file;
    /// Music's old files come back under their names, bytes and times, on their tracks, not trashed; the copies' rows trashed and
    /// holding no track; the trash tokens kept on the entry. (2) A booklet dropped in since stays beside the old files. (3) A copied
    /// file rewritten since refuses it whole, naming it, nothing moved (OQ-D). (4) A second tap is refused, and nothing moves. (5) A
    /// later Replace from Incoming took the copies' place: refused, the later album and its own Undo untouched. Never a copy moved
    /// into Bob's folder, a file of Bob's touched, a file deleted outright, an old file put back over a file at its name.
    #[tokio::test]
    async fn undo_of_a_replace_from_a_copy_puts_musics_copy_back_and_leaves_bobs() {
        for case in 1..=5 {
            let (svc, music, inc, ann, bob) = with_three_own(&format!("ry01-undo{case}"), RY01_DIR, RY01_DIR, Some("2019"), 2, 2).await;
            let (before, theirs) = (lying(&music.join(RY01_DIR)), lying(&bob));
            let c = svc.lib.catalogue();
            let olds: Vec<Option<i64>> = (1..=2).map(|n| c.track_of(0, &ry01_file(RY01_DIR, n, "mp3"))).collect();
            let at = bobs_copy(&svc, &bob, RY01_DIR).await;
            let done = svc.replacing_from_copy(&at, (0, RY01_DIR.into()), None, None).await.unwrap();
            let key = done.undo.clone().unwrap();
            let id: EventId = key.parse().unwrap();
            let new = music.join(&done.folder);
            let copied: Vec<String> = lying(&new).into_keys().collect();
            let mut later = None;
            match case {
                2 => std::fs::write(new.join("booklet.pdf"), b"pdf").unwrap(),
                3 => std::fs::write(new.join(ry01_file("", 2, "flac")), b"two, retagged").unwrap(),
                5 => {
                    // A later Replace from Incoming: a 24/96 copy takes the copies' place, and they go into Music's trash.
                    std::fs::create_dir_all(inc.join(RY01_DIR)).unwrap();
                    for n in 1..=2 {
                        std::fs::write(inc.join(ry01_file(RY01_DIR, n, "flac")), format!("hi-res {n}")).unwrap();
                    }
                    c.walk_all();
                    c.pretend_read_for_tests();
                    c.identify().unwrap();
                    let token = format!("1700000000/{RY01_DIR}");
                    let event = Event::AlbumReplaced {
                        source: "cratarr".into(),
                        album_id: "al-1".into(),
                        title: "Zebra".into(),
                        from_dir: (1, RY01_DIR.into()),
                        to_dir: (0, RY01_DIR.into()),
                        moves: (1..=2)
                            .map(|n| {
                                let rel = ry01_file(RY01_DIR, n, "flac");
                                TrackMove { track: c.track_of(0, &rel), from: (1, rel.clone()), to: (0, rel.clone()), was: c.track_of(1, &rel) }
                            })
                            .collect(),
                        replaced: files_under(&new)
                            .into_iter()
                            .map(|f| {
                                let rel = format!("{RY01_DIR}/{f}");
                                TrackMove { track: c.track_of(0, &rel), from: (0, rel), to: (0, format!("{}/{token}/{f}", crate::trash::TRASH_DIR)), was: None }
                            })
                            .collect(),
                        token,
                        release_group: None,
                        converted: Vec::new(),
                    };
                    let later_id = svc.journal.intend(epoch_now(), &event).unwrap();
                    crate::moves::apply(&svc.steps(&event).unwrap()).unwrap();
                    svc.finish_replace(&event, false, false).await;
                    svc.journal.applied(later_id).unwrap();
                    later = Some(later_id);
                }
                _ => {}
            }
            let lies = lying(&music);
            let undone = svc.undo(&key).await;
            match case {
                3 => {
                    let e = undone.unwrap_err().to_string();
                    assert!(e.contains(&ry01_file("", 2, "flac")) && e.contains("changed since"), "it names the file: {e}");
                    assert_eq!(lying(&music), lies, "refused whole: nothing moved");
                    assert!(svc.journal.get(id).unwrap().is_some_and(|e| !e.undone));
                }
                5 => {
                    let e = undone.unwrap_err().to_string();
                    assert!(e.contains("None of the files copied"), "{e}");
                    assert_eq!(lying(&music), lies, "the later Replace's files untouched");
                    svc.undo(&later.unwrap().to_string()).await.unwrap();
                    assert_eq!(std::fs::read(inc.join(ry01_file(RY01_DIR, 1, "flac"))).unwrap(), b"hi-res 1", "its own Undo works after");
                    assert_eq!(std::fs::read(new.join(ry01_file("", 1, "flac"))).unwrap(), std::fs::read(bob.join(ry01_file(RY01_DIR, 1, "flac"))).unwrap());
                }
                _ => {
                    undone.unwrap();
                    let mut back = lying(&music.join(RY01_DIR));
                    if case == 2 {
                        assert_eq!(back.remove("booklet.pdf").map(|(b, _)| b), Some(b"pdf".to_vec()), "what came after stays");
                    }
                    assert_eq!(back, before, "Music's old copy back as it was, bytes and times");
                    let trash: std::collections::BTreeSet<Vec<u8>> = lying(&music.join(crate::trash::TRASH_DIR)).into_values().map(|(b, _)| b).collect();
                    assert_eq!(trash, theirs.values().map(|(b, _)| b.clone()).collect(), "the copies in Music's trash, never deleted");
                    for (n, t) in (1..=2).zip(&olds) {
                        let rel = ry01_file(RY01_DIR, n, "mp3");
                        assert_eq!(row_at(&c, 0, &rel), Some((*t, None)), "{rel}: back on its track, not trashed");
                    }
                    let tokens = uncopied(&svc, id);
                    assert_eq!(tokens.len(), copied.len(), "every copy's token kept on the entry: {tokens:?}");
                    for t in tokens.iter().filter(|t| t.ends_with(".flac")) {
                        let row = row_at(&c, 0, &format!("{}/{t}", crate::trash::TRASH_DIR)).expect("the copy's row in the trash");
                        assert!(row.0.is_none() && row.1.is_some(), "{t}: trashed, holding no track");
                    }
                    if case == 4 {
                        let after = lying(&music);
                        assert!(svc.undo(&key).await.unwrap_err().to_string().contains("already"), "undone once");
                        assert_eq!(lying(&music), after, "nothing moves");
                    }
                }
            }
            assert_eq!(lying(&bob), theirs, "Bob's untouched by the Undo");
            for d in [music, inc, ann, bob] {
                std::fs::remove_dir_all(d).ok();
            }
        }
    }

    /// RF7 (the review of runs 11 and 12, RA3): after a Replace from Bob's copy, a tag fix rewrote a copied FLAC in Music; RY01's Undo
    /// is refused, saying only what is so: which file changed, and that the library's old copy cannot come back; no advice. Then the
    /// fix is undone as 0092's Undo does it (the copy's bytes again, written anew: a new time), and the Undo says the same, which is
    /// still so. Each time nothing moves, the entry is not undone and no Undo is begun on it. Never advice that cannot work ("undo
    /// that change first").
    #[tokio::test]
    async fn the_undo_of_a_replace_from_a_changed_copy_says_only_what_is_so() {
        let (svc, music, inc, ann, bob) = with_three_own("rf7-changed", RY01_DIR, RY01_DIR, Some("2019"), 2, 2).await;
        let at = bobs_copy(&svc, &bob, RY01_DIR).await;
        let done = svc.replacing_from_copy(&at, (0, RY01_DIR.into()), None, None).await.unwrap();
        let key = done.undo.clone().unwrap();
        let id: EventId = key.parse().unwrap();
        let file = music.join(&done.folder).join(ry01_file("", 2, "flac"));
        let copied = std::fs::read(&file).unwrap();
        let said = format!("{} has changed since Zebra was copied in, so that library's old copy cannot come back", ry01_file("", 2, "flac"));
        std::fs::write(&file, b"flac number 2, its tag written").unwrap();
        for fix_undone in [false, true] {
            if fix_undone {
                std::fs::write(&file, &copied).unwrap();
                let later = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
                std::fs::File::options().write(true).open(&file).unwrap().set_modified(later).unwrap();
                assert_eq!(std::fs::read(&file).unwrap(), copied, "the copy's bytes again");
            }
            let lies = lying(&music);
            let e = svc.undo(&key).await.unwrap_err().to_string();
            assert_eq!(e, said, "the fix undone {fix_undone}: what is so");
            assert!(!e.contains("undo") && !e.contains("first") && !e.contains("delete"), "no advice: {e}");
            assert_eq!(lying(&music), lies, "the fix undone {fix_undone}: nothing moved");
            assert!(svc.journal.get(id).unwrap().is_some_and(|e| e.applied && !e.undone), "not undone");
            assert!(svc.journal.result(id, UNCOPIED).unwrap().is_none(), "no Undo begun on it");
        }
        for d in [music, inc, ann, bob] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// Files as the disk has them, but a plan of moves stops after its first step with an error, that step done (the disk gone away
    /// midway, RF9). Moved back as the disk does.
    struct MovesHalf;

    impl crate::ports::Files for MovesHalf {
        fn trash(&self, folder: &std::path::Path, rel: &str, now_epoch: i64) -> anyhow::Result<String> {
            crate::trash::trash(folder, rel, now_epoch)
        }
        fn restore(&self, folder: &std::path::Path, token: &str) -> anyhow::Result<String> {
            crate::trash::restore(folder, token)
        }
        fn purge(&self, folder: &std::path::Path, now_epoch: i64, keep_days: u32) -> anyhow::Result<usize> {
            crate::trash::purge(folder, now_epoch, keep_days)
        }
        fn can_write(&self, _folder: &std::path::Path) -> bool {
            true
        }
        fn move_all(&self, plan: &[crate::moves::Step]) -> anyhow::Result<()> {
            crate::moves::apply(&plan[..plan.len().min(1)])?;
            anyhow::bail!("the disk went away")
        }
        fn move_back(&self, plan: &[crate::moves::Step]) -> anyhow::Result<()> {
            crate::moves::revert(plan)
        }
        fn clear_parts(&self, plan: &[crate::moves::Step]) {
            crate::moves::clear_parts(plan)
        }
        fn copy_all(&self, plan: &[crate::moves::Step]) -> anyhow::Result<Vec<bool>> {
            crate::moves::copy(plan)
        }
        fn shares(&self, _from: &std::path::Path, _to: &std::path::Path) -> bool {
            false
        }
    }

    /// RF9 (the review of runs 11 and 12, RA7), Copy to: an Undo cut short is finished by the pass, with no second tap; one of which
    /// nothing happened is forgotten; one that fails midway, or that cannot be finished, puts back what it took. (1) Cut short after
    /// 1.flac reached the trash under the Undo's second: the pass takes 2.flac too, both rows trashed, no folder left, the entry undone
    /// once, its tokens kept. (2) Its second written, nothing moved: the pass forgets it, the copy whole and not undone; a tap then
    /// undoes it. (3) The disk failing after the tap's first move: refused, the copy whole again, no Undo begun, and the pass changes
    /// nothing after. (4) Cut short, 2.flac rewritten since: the pass cannot finish it, and 1.flac comes back beside it. Never a short
    /// album left in own music; Music's original as it was.
    #[tokio::test]
    async fn the_pass_finishes_a_copys_undo_cut_short_or_puts_back_what_it_took() {
        for case in 1..=4 {
            let (svc, music, inc, own) = with_own_music(&format!("rf9-copy{case}")).await;
            let before = lying(&music);
            let at = svc.locate_listed("cratarr", copy_at("al-1", 0, &music.to_string_lossy(), "A/Album")).await.unwrap();
            let undo = svc.copying(&at, 2, None, None).await.unwrap().undo.unwrap();
            let svc = if case == 3 { IncomingService { files: Arc::new(MovesHalf), ..svc } } else { svc };
            let (id, stamp): (EventId, i64) = (undo.parse().unwrap(), 1_700_000_000);
            if case == 4 {
                std::fs::write(own.join("A/Album/2.flac"), b"two, retagged").unwrap();
            }
            let whole = lying(&own);
            if case != 3 {
                svc.journal.done(id, UNCOPIED, stamp, &serde_json::json!({ "stamp": stamp, "tokens": [] })).unwrap();
            }
            if matches!(case, 1 | 4) {
                crate::trash::trash_file(&own, "A/Album/1.flac", stamp).unwrap();
            }
            if case == 3 {
                assert!(svc.undo(&undo).await.is_err(), "the disk failing midway: refused");
            }
            svc.reconcile().await;
            let c = svc.lib.catalogue();
            let entry = svc.journal.get(id).unwrap().unwrap();
            if case == 1 {
                assert!(!own.join("A/Album").exists(), "the other file went too, no folder left");
                let trash = lying(&own.join(crate::trash::TRASH_DIR));
                assert_eq!(trash.into_keys().collect::<Vec<_>>(), [format!("{stamp}/A/Album/1.flac"), format!("{stamp}/A/Album/2.flac")], "one trash entry each");
                for f in ["1.flac", "2.flac"] {
                    assert!(c.has_row(2, &format!("{}/{stamp}/A/Album/{f}", crate::trash::TRASH_DIR)), "{f}: its row trashed");
                }
                assert!(entry.undone && uncopied(&svc, id).len() == 2, "undone, its tokens kept");
                assert!(svc.undo(&undo).await.is_err(), "undone once");
            } else {
                assert_eq!(lying(&own), whole, "{case}: the copy whole as before the Undo, nothing in the trash");
                assert!(c.has_row(2, "A/Album/1.flac") && c.has_row(2, "A/Album/2.flac"), "{case}: its rows at their places");
                assert!(!entry.undone && svc.journal.result(id, UNCOPIED).unwrap().is_none(), "{case}: not undone, no Undo begun");
                if case == 2 {
                    svc.undo(&undo).await.unwrap();
                    assert!(!own.join("A/Album").exists(), "a tap then undoes it");
                }
            }
            assert_eq!(lying(&music), before, "{case}: Music's original as it was");
            for d in [music, inc, own] {
                std::fs::remove_dir_all(d).ok();
            }
        }
    }

    /// RF9 (RA7), a Replace from Bob's copy: its Undo, tapped thirteen days on, cut short between its halves (every copy in Music's
    /// trash under the Undo's second, Music's old files still in the trash under RY01's: Music holds neither copy). (1) The pass
    /// finishes it with no tap: Music's old copy back, bytes and times, on its tracks; the copies in Music's trash, their rows trashed
    /// holding no track; the entry undone, every token kept. (2) The old files purged from the trash meanwhile (RY01's second past the
    /// trash days): the pass puts the copies back, bytes and times, on the record's tracks; the entry not undone, no Undo begun. (3)
    /// The disk failing after the tap's first move: refused, the copies back in Music, Music's old files still in its trash, the entry
    /// not undone, no Undo begun; the pass changes nothing after. Never Music with neither copy; Bob's copy never touched.
    #[tokio::test]
    async fn the_pass_finishes_a_replace_from_a_copys_undo_cut_short_and_music_never_holds_neither() {
        for case in 1..=3 {
            let (svc, music, inc, ann, bob) = with_three_own(&format!("rf9-ry01-{case}"), RY01_DIR, RY01_DIR, Some("2019"), 2, 2).await;
            let (before, theirs) = (lying(&music.join(RY01_DIR)), lying(&bob));
            let c = svc.lib.catalogue();
            let olds: Vec<Option<i64>> = (1..=2).map(|n| c.track_of(0, &ry01_file(RY01_DIR, n, "mp3"))).collect();
            let at = bobs_copy(&svc, &bob, RY01_DIR).await;
            let done = svc.replacing_from_copy(&at, (0, RY01_DIR.into()), None, None).await.unwrap();
            let (key, new) = (done.undo.clone().unwrap(), music.join(&done.folder));
            let id: EventId = key.parse().unwrap();
            let copies = lying(&new);
            let svc = if case == 3 { IncomingService { files: Arc::new(MovesHalf), ..svc } } else { svc };
            let stamp = epoch_now() + 13 * 86_400;
            if case != 3 {
                svc.journal.done(id, UNCOPIED, stamp, &serde_json::json!({ "stamp": stamp, "tokens": [] })).unwrap();
                for f in copies.keys() {
                    crate::trash::trash_file(&music, &format!("{}/{f}", done.folder), stamp).unwrap();
                }
                assert!(lying(&new).is_empty(), "{case}: Music holds neither copy, for now");
            }
            if case == 2 {
                crate::trash::purge(&music, epoch_now() + 15 * 86_400, 14).unwrap();
            }
            if case == 3 {
                assert!(svc.undo(&key).await.is_err(), "the disk failing midway: refused");
            }
            svc.reconcile().await;
            let entry = svc.journal.get(id).unwrap().unwrap();
            if case == 1 {
                assert_eq!(lying(&music.join(RY01_DIR)), before, "Music's old copy back, bytes and times");
                for (n, t) in (1..=2).zip(&olds) {
                    assert_eq!(row_at(&c, 0, &ry01_file(RY01_DIR, n, "mp3")), Some((*t, None)), "on its track, not trashed");
                }
                let trash: std::collections::BTreeSet<Vec<u8>> = lying(&music.join(crate::trash::TRASH_DIR)).into_values().map(|(b, _)| b).collect();
                assert_eq!(trash, theirs.values().map(|(b, _)| b.clone()).collect(), "the copies in Music's trash");
                let tokens = uncopied(&svc, id);
                assert!(entry.undone && tokens.len() == copies.len(), "undone, every token kept: {tokens:?}");
                for t in tokens.iter().filter(|t| t.ends_with(".flac")) {
                    let row = row_at(&c, 0, &format!("{}/{t}", crate::trash::TRASH_DIR)).expect("the copy's row in the trash");
                    assert!(row.0.is_none() && row.1.is_some(), "{t}: trashed, holding no track");
                }
            } else {
                assert_eq!(lying(&new), copies, "{case}: the copies back in Music, bytes and times");
                for (n, t) in (1..=2).zip(&olds) {
                    assert_eq!(row_at(&c, 0, &ry01_file(&done.folder, n, "flac")), Some((*t, None)), "{case}: on the record's track");
                }
                assert!(!entry.undone && svc.journal.result(id, UNCOPIED).unwrap().is_none(), "{case}: not undone, no Undo begun");
                if case == 3 {
                    assert_eq!(lying(&music.join(crate::trash::TRASH_DIR)).len(), before.len(), "Music's old files still in its trash");
                }
            }
            assert_eq!(lying(&bob), theirs, "{case}: Bob's copy never touched");
            for d in [music, inc, ann, bob] {
                std::fs::remove_dir_all(d).ok();
            }
        }
    }

    /// Files as the disk has them, noting at each plan of moves whether the copies are held on their records then
    /// (`Catalogue::copies_are_moving`, RF15).
    struct Held(Arc<crate::catalogue::Catalogue>, Arc<std::sync::Mutex<Vec<bool>>>);

    impl crate::ports::Files for Held {
        fn trash(&self, folder: &std::path::Path, rel: &str, now_epoch: i64) -> anyhow::Result<String> {
            crate::trash::trash(folder, rel, now_epoch)
        }
        fn restore(&self, folder: &std::path::Path, token: &str) -> anyhow::Result<String> {
            crate::trash::restore(folder, token)
        }
        fn purge(&self, folder: &std::path::Path, now_epoch: i64, keep_days: u32) -> anyhow::Result<usize> {
            crate::trash::purge(folder, now_epoch, keep_days)
        }
        fn can_write(&self, _folder: &std::path::Path) -> bool {
            true
        }
        fn move_all(&self, plan: &[crate::moves::Step]) -> anyhow::Result<()> {
            self.1.lock().unwrap().push(self.0.copies_are_moving());
            crate::moves::apply(plan)
        }
        fn move_back(&self, plan: &[crate::moves::Step]) -> anyhow::Result<()> {
            crate::moves::revert(plan)
        }
        fn clear_parts(&self, plan: &[crate::moves::Step]) {
            crate::moves::clear_parts(plan)
        }
        fn copy_all(&self, plan: &[crate::moves::Step]) -> anyhow::Result<Vec<bool>> {
            crate::moves::copy(plan)
        }
        fn shares(&self, _from: &std::path::Path, _to: &std::path::Path) -> bool {
            false
        }
    }

    /// RF15 (the review of runs 11 and 12, RA6): the Undo of a Copy to and the Undo of a Replace from Bob's copy hold the copies on
    /// their records while their files move (`Catalogue::copies_moving`, as Keep's and Replace's Undos do), so a pass meanwhile lets
    /// no copy half way leave its record: every plan of moves an Undo makes is made with the hold on.
    #[tokio::test]
    async fn the_undos_of_copies_hold_the_copies_on_their_records_while_they_move() {
        let seen: Arc<std::sync::Mutex<Vec<bool>>> = Arc::default();
        let (svc, music, inc, own) = with_own_music("rf15-held-copy").await;
        let cat = svc.lib.catalogue();
        let svc = IncomingService { files: Arc::new(Held(cat, seen.clone())), ..svc };
        let at = svc.locate_listed("cratarr", copy_at("al-1", 0, &music.to_string_lossy(), "A/Album")).await.unwrap();
        let undo = svc.copying(&at, 2, None, None).await.unwrap().undo.unwrap();
        seen.lock().unwrap().clear();
        svc.undo(&undo).await.unwrap();
        let held = seen.lock().unwrap().clone();
        assert!(!held.is_empty() && held.iter().all(|h| *h), "Copy to's Undo moves with the copies held: {held:?}");
        assert!(!own.join("A/Album").exists(), "and it is undone");
        for d in [music, inc, own] {
            std::fs::remove_dir_all(d).ok();
        }
        let (svc, music, inc, ann, bob) = with_three_own("rf15-held-ry01", RY01_DIR, RY01_DIR, Some("2019"), 2, 2).await;
        let before = lying(&music.join(RY01_DIR));
        let cat = svc.lib.catalogue();
        let svc = IncomingService { files: Arc::new(Held(cat, seen.clone())), ..svc };
        let at = bobs_copy(&svc, &bob, RY01_DIR).await;
        let done = svc.replacing_from_copy(&at, (0, RY01_DIR.into()), None, None).await.unwrap();
        seen.lock().unwrap().clear();
        svc.undo(&done.undo.unwrap()).await.unwrap();
        let held = seen.lock().unwrap().clone();
        assert!(!held.is_empty() && held.iter().all(|h| *h), "RY01's Undo moves with the copies held: {held:?}");
        assert_eq!(lying(&music.join(RY01_DIR)), before, "and Music's old copy is back");
        for d in [music, inc, ann, bob] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// The refusal of the one of two at once that did not go, and the other's answer.
    fn one_of_two<T>(a: Result<T, ToggleError>, b: Result<T, ToggleError>) -> (T, String) {
        match (a, b) {
            (Ok(done), Err(e)) | (Err(e), Ok(done)) => (done, e.to_string()),
            (Ok(_), Ok(_)) => panic!("both went"),
            (Err(x), Err(y)) => panic!("neither went: {x}; {y}"),
        }
    }

    /// RF15 (RA16): taps on one record's copy in one library go one at a time; the other is refused at once, saying what is going on.
    /// (1) Two Copy to taps of Music's album into Ann's music at once: one copies, the other is refused ("Zebra is being copied"); one
    /// journal entry, one copy, the original's files alone. (2) That copy's Undo and another Copy to of it into Ann's music at once: one
    /// goes, the other is refused saying what is going on. (3) Two Undo taps of a Replace from Bob's copy at once: one goes, the other is
    /// refused ("Zebra is being undone"); Music's trash holds the copies once. Never two entries or two copies from two taps, nor a copy
    /// under a second no entry names.
    #[tokio::test]
    async fn taps_on_one_records_copy_go_one_at_a_time() {
        let (svc, music, inc, own) = with_own_music("rf15-taps").await;
        let listed = || copy_at("al-1", 0, &music.to_string_lossy(), "A/Album");
        let (a, b) = (svc.locate_listed("cratarr", listed()).await.unwrap(), svc.locate_listed("cratarr", listed()).await.unwrap());
        let (x, y) = tokio::join!(svc.copying(&a, 2, None, None), svc.copying(&b, 2, None, None));
        let (done, refused) = one_of_two(x, y);
        assert_eq!(refused, "Zebra is being copied", "(1) the other told what is going on");
        assert_eq!(svc.journal.album_events_since(0).unwrap().len(), 1, "(1) one entry");
        assert_eq!(lying(&own), lying(&music), "(1) one copy, the original's files alone");
        let key = done.undo.unwrap();
        let c = svc.locate_listed("cratarr", listed()).await.unwrap();
        let (x, y) = tokio::join!(async { svc.undo(&key).await.map(|_| ()) }, async { svc.copying(&c, 2, None, None).await.map(|_| ()) });
        let ((), refused) = one_of_two(x, y);
        assert!(refused == "Zebra is being undone" || refused == "Zebra is being copied", "(2) the other told what is going on: {refused}");
        assert_eq!(svc.journal.album_events_since(0).unwrap().len(), 1, "(2) no second entry");
        for d in [music, inc, own] {
            std::fs::remove_dir_all(d).ok();
        }
        let (svc, music, inc, ann, bob) = with_three_own("rf15-taps-ry01", RY01_DIR, RY01_DIR, Some("2019"), 2, 2).await;
        let at = bobs_copy(&svc, &bob, RY01_DIR).await;
        let key = svc.replacing_from_copy(&at, (0, RY01_DIR.into()), None, None).await.unwrap().undo.unwrap();
        let copies = lying(&music.join(RY01_DIR)).len();
        let (x, y) = tokio::join!(svc.undo(&key), svc.undo(&key));
        let (_, refused) = one_of_two(x, y);
        assert_eq!(refused, "Zebra is being undone", "(3) the other told what is going on");
        assert_eq!(lying(&music.join(crate::trash::TRASH_DIR)).len(), copies, "(3) the copies in Music's trash once");
        for d in [music, inc, ann, bob] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// The tokens a Copy to's Undo kept on its entry (T1).
    fn uncopied(svc: &IncomingService, id: EventId) -> Vec<String> {
        svc.journal.result(id, UNCOPIED).unwrap().and_then(|v| serde_json::from_value(v["tokens"].clone()).ok()).unwrap_or_default()
    }

    /// T4.1: a Copy to whose rows are refused once, after its files are written (a catalogue refusing the copies' rows): the tap
    /// fails with its reason, the intent is kept and the copies stand; the pass then finishes it, its rows copies on the record's
    /// tracks, the entry applied, and its Undo works.
    #[tokio::test]
    async fn a_copy_whose_rows_fail_is_finished_by_the_pass() {
        let (svc, music, inc, own) = with_own_music("t4-rows").await;
        let at = svc.locate_listed("cratarr", copy_at("al-1", 0, &music.to_string_lossy(), "A/Album")).await.unwrap();
        let c = svc.lib.catalogue();
        c.execute_for_tests("CREATE TEMP TRIGGER refuse BEFORE INSERT ON files WHEN NEW.folder = 2 BEGIN SELECT RAISE(ABORT, 'refused once'); END");
        let e = svc.copying(&at, 2, None, None).await.unwrap_err();
        assert!(e.to_string().contains("refused once"), "the tap fails with its reason: {e}");
        let intents: Vec<EventId> = svc.journal.intents().unwrap().into_iter().filter(|e| matches!(e.event, Event::AlbumCopied { .. })).map(|e| e.id).collect();
        assert_eq!(intents.len(), 1, "its intent kept");
        assert!(own.join("A/Album/1.flac").is_file() && own.join("A/Album/2.flac").is_file(), "the copies stand");
        c.execute_for_tests("DROP TRIGGER refuse");
        svc.reconcile().await;
        assert!(c.track_of(2, "A/Album/1.flac").is_some() && c.track_of(2, "A/Album/1.flac") == c.track_of(0, "A/Album/1.flac"), "copies on the record's tracks");
        assert!(svc.journal.album_events_since(0).unwrap().iter().any(|(id, _, e, undone)| *id == intents[0] && matches!(e, Event::AlbumCopied { .. }) && !undone), "applied");
        svc.undo(&intents[0].to_string()).await.unwrap();
        assert!(!own.join("A/Album/1.flac").exists(), "and its Undo takes it away");
        for d in [music, inc, own] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// Files as the disk has them, but a copy fails (T4.2): all or none, as `Files::copy_all` promises, nothing written.
    struct CopyFails(Arc<dyn crate::ports::Files>);

    impl crate::ports::Files for CopyFails {
        fn trash(&self, folder: &std::path::Path, rel: &str, now_epoch: i64) -> anyhow::Result<String> {
            self.0.trash(folder, rel, now_epoch)
        }
        fn restore(&self, folder: &std::path::Path, token: &str) -> anyhow::Result<String> {
            self.0.restore(folder, token)
        }
        fn purge(&self, folder: &std::path::Path, now_epoch: i64, keep_days: u32) -> anyhow::Result<usize> {
            self.0.purge(folder, now_epoch, keep_days)
        }
        fn can_write(&self, folder: &std::path::Path) -> bool {
            self.0.can_write(folder)
        }
        fn move_all(&self, plan: &[crate::moves::Step]) -> anyhow::Result<()> {
            self.0.move_all(plan)
        }
        fn move_back(&self, plan: &[crate::moves::Step]) -> anyhow::Result<()> {
            self.0.move_back(plan)
        }
        fn clear_parts(&self, plan: &[crate::moves::Step]) {
            self.0.clear_parts(plan)
        }
        fn copy_all(&self, _plan: &[crate::moves::Step]) -> anyhow::Result<Vec<bool>> {
            anyhow::bail!("the second file could not be copied")
        }
        fn shares(&self, from: &std::path::Path, to: &std::path::Path) -> bool {
            self.0.shares(from, to)
        }
    }

    /// T4.2: a Copy to failing while it copies its files leaves nothing under the target, no part, and no intent (as before T4).
    #[tokio::test]
    async fn a_copy_that_fails_while_copying_leaves_nothing_and_no_intent() {
        let (svc, music, inc, own) = with_own_music("t4-files").await;
        let svc = IncomingService { files: Arc::new(CopyFails(crate::ports::disk())), ..svc };
        let at = svc.locate_listed("cratarr", copy_at("al-1", 0, &music.to_string_lossy(), "A/Album")).await.unwrap();
        assert!(svc.copying(&at, 2, None, None).await.is_err());
        assert!(lying(&own).is_empty(), "nothing under the target, no part");
        assert!(svc.journal.intents().unwrap().is_empty(), "no intent");
        for d in [music, inc, own] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// T4.3: a restart between a Copy to's files and its rows (the intent in the journal, both files copied, no rows): the pass
    /// makes the rows as copies, applies it, and its Undo works.
    #[tokio::test]
    async fn a_copy_stopped_before_its_rows_is_finished_by_the_pass() {
        let (svc, music, inc, own) = with_own_music("t4-restart").await;
        let at = svc.locate_listed("cratarr", copy_at("al-1", 0, &music.to_string_lossy(), "A/Album")).await.unwrap();
        let (_, plan) = svc.plan_move(&at, 2, None).await.unwrap();
        let plan = plan.unwrap();
        let c = svc.lib.catalogue();
        let from = |rel: &str| format!("A/Album/{rel}");
        let copies: Vec<TrackMove> = plan.moves.iter().map(|(rel, to)| TrackMove { track: c.track_of(0, &from(rel)), from: (0, from(rel)), to: (2, to.clone()), was: None }).collect();
        let event = Event::AlbumCopied { source: "cratarr".into(), album_id: "al-1".into(), title: "Zebra".into(), from_dir: (0, "A/Album".into()), to_dir: (2, plan.folder.clone()), copies };
        let underway = svc.intend(epoch_now(), &event).unwrap();
        let id = underway.id();
        svc.files.copy_all(&svc.steps(&event).unwrap()).unwrap();
        drop(underway);
        assert!(!c.has_row(2, "A/Album/1.flac"), "no rows yet");
        svc.reconcile().await;
        assert!(svc.journal.get(id).unwrap().is_some_and(|e| e.applied), "applied");
        assert!(c.track_of(2, "A/Album/1.flac").is_some() && c.track_of(2, "A/Album/1.flac") == c.track_of(0, "A/Album/1.flac"), "rows made as copies");
        svc.undo(&id.to_string()).await.unwrap();
        assert!(!own.join("A/Album/1.flac").exists(), "its Undo works");
        for d in [music, inc, own] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// T1.1, files added since: a Copy to's Undo takes only the files it copied, still as copied, and leaves a cover added to the
    /// copy's folder since; the copied ones go to the trash, their rows trashed with their tokens, kept on the entry. Music's
    /// original as it was; the folder never trashed whole.
    #[tokio::test]
    async fn undo_takes_only_the_copied_files() {
        let (svc, music, inc, own) = with_own_music("t1-added").await;
        let before = lying(&music);
        let at = svc.locate_listed("cratarr", copy_at("al-1", 0, &music.to_string_lossy(), "A/Album")).await.unwrap();
        let undo = svc.copying(&at, 2, None, None).await.unwrap().undo.unwrap();
        std::fs::write(own.join("A/Album/cover.jpg"), b"jpg").unwrap();
        svc.undo(&undo).await.unwrap();
        assert_eq!(lying(&own.join("A/Album")).into_keys().collect::<Vec<_>>(), ["cover.jpg"], "what came since stays");
        let trash: Vec<Vec<u8>> = lying(&own.join(crate::trash::TRASH_DIR)).into_values().map(|(bytes, _)| bytes).collect();
        assert_eq!(trash, [b"one".to_vec(), b"two".to_vec()], "both copied files into the trash");
        let id: EventId = undo.parse().unwrap();
        let tokens = uncopied(&svc, id);
        assert!(tokens.len() == 2 && tokens[0].ends_with("/A/Album/1.flac") && tokens[1].ends_with("/A/Album/2.flac"), "where they went, on the entry: {tokens:?}");
        let c = svc.lib.catalogue();
        assert!(!c.has_row(2, "A/Album/1.flac") && !c.has_row(2, "A/Album/2.flac"), "the copies' rows gone from their place");
        for t in &tokens {
            assert!(c.has_row(2, &format!("{}/{t}", crate::trash::TRASH_DIR)), "trashed with its token: {t}");
        }
        assert!(svc.journal.get(id).unwrap().is_some_and(|e| e.undone));
        assert_eq!(lying(&music), before, "Music's original untouched");
        for d in [music, inc, own] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// T1.1 as T1c has it (OQ-D): a copied file rewritten since refuses a Copy to's Undo whole, naming
    /// the file and saying to delete the copy instead. Nothing goes to the trash: the copied file, the rewritten one and a cover
    /// added since all in place, the rows as they were, the entry not undone. Never a short album; never a file not as copied taken.
    #[tokio::test]
    async fn undo_of_a_copy_rewritten_since_is_refused_whole() {
        let (svc, music, inc, own) = with_own_music("t1c-rewritten").await;
        let before = lying(&music);
        let at = svc.locate_listed("cratarr", copy_at("al-1", 0, &music.to_string_lossy(), "A/Album")).await.unwrap();
        let undo = svc.copying(&at, 2, None, None).await.unwrap().undo.unwrap();
        std::fs::write(own.join("A/Album/cover.jpg"), b"jpg").unwrap();
        std::fs::write(own.join("A/Album/2.flac"), b"two, retagged").unwrap();
        let c = svc.lib.catalogue();
        let rows = || -> Vec<String> {
            let db = c.writer_for_tests();
            let mut st = db.prepare("SELECT rel || ' ' || ifnull(track, '') || ' ' || ifnull(trashed, '') || ' ' || size || ' ' || mtime FROM files WHERE folder = 2 ORDER BY rel").unwrap();
            let out = st.query_map([], |r| r.get::<_, String>(0)).unwrap().map(Result::unwrap).collect();
            out
        };
        let (lies, held) = (lying(&own), rows());
        let e = svc.undo(&undo).await.unwrap_err().to_string();
        assert!(e.contains("2.flac") && e.contains("delete the copy"), "it names the file, and the way then: {e}");
        assert_eq!(lying(&own.join("A/Album")).into_keys().collect::<Vec<_>>(), ["1.flac", "2.flac", "cover.jpg"], "all in place");
        assert!(lying(&own.join(crate::trash::TRASH_DIR)).is_empty(), "nothing in the trash");
        assert_eq!(lying(&own), lies, "nothing moved or written");
        assert_eq!(rows(), held, "the rows as they were");
        assert!(c.has_row(2, "A/Album/1.flac") && c.has_row(2, "A/Album/2.flac"));
        let id: EventId = undo.parse().unwrap();
        assert!(svc.journal.get(id).unwrap().is_some_and(|e| e.applied && !e.undone), "the entry not undone");
        assert!(uncopied(&svc, id).is_empty(), "no trash token on it");
        assert_eq!(lying(&music), before, "Music's original untouched");
        for d in [music, inc, own] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// Files as the disk has them, every copy said to share its original's blocks, as on the NAS's btrfs (AL01's rooms).
    struct CopyShares(Arc<dyn crate::ports::Files>);

    impl crate::ports::Files for CopyShares {
        fn trash(&self, folder: &std::path::Path, rel: &str, now_epoch: i64) -> anyhow::Result<String> {
            self.0.trash(folder, rel, now_epoch)
        }
        fn restore(&self, folder: &std::path::Path, token: &str) -> anyhow::Result<String> {
            self.0.restore(folder, token)
        }
        fn purge(&self, folder: &std::path::Path, now_epoch: i64, keep_days: u32) -> anyhow::Result<usize> {
            self.0.purge(folder, now_epoch, keep_days)
        }
        fn can_write(&self, folder: &std::path::Path) -> bool {
            self.0.can_write(folder)
        }
        fn move_all(&self, plan: &[crate::moves::Step]) -> anyhow::Result<()> {
            self.0.move_all(plan)
        }
        fn move_back(&self, plan: &[crate::moves::Step]) -> anyhow::Result<()> {
            self.0.move_back(plan)
        }
        fn clear_parts(&self, plan: &[crate::moves::Step]) {
            self.0.clear_parts(plan)
        }
        fn copy_all(&self, plan: &[crate::moves::Step]) -> anyhow::Result<Vec<bool>> {
            self.0.copy_all(plan).map(|copied| vec![true; copied.len()])
        }
        fn shares(&self, from: &std::path::Path, to: &std::path::Path) -> bool {
            self.0.shares(from, to)
        }
    }

    /// AL01.5's (e), as T1c has it (refused whole): root's Copy to of a 10-file album into bob's own music,
    /// every copy sharing Music's blocks; a cover fix rewrote 3 of the copies since, and a file was added to the folder. Bob's room
    /// counts the 3 rewritten (their own bytes now) and the added file, never the 7 still Music's bytes. Root's Undo is refused
    /// whole: all ten copies and the added file in place, nothing in the trash, his room as it was. Never a short album left in
    /// bob's music by the Undo.
    #[tokio::test]
    async fn a_copys_undo_refused_whole_leaves_room_counting_what_is_its_own() {
        let (music, inc, own) = (folder("al01-undo-music"), folder("al01-undo-inc"), folder("al01-undo-bob"));
        let svc = on_disk(&music, &inc).await;
        let svc = IncomingService { files: Arc::new(CopyShares(crate::ports::disk())), ..svc };
        let c = svc.lib.catalogue();
        c.set_folders(vec![music.clone(), inc.clone(), own.clone()]).unwrap();
        std::fs::create_dir_all(music.join("A/Album")).unwrap();
        for n in 1..=10 {
            std::fs::write(music.join(format!("A/Album/{n:02}.flac")), format!("track {n}")).unwrap();
        }
        c.walk_all();
        c.pretend_read_for_tests();
        c.identify().unwrap();
        let at = svc.locate_listed("cratarr", copy_at("al-1", 0, &music.to_string_lossy(), "A/Album")).await.unwrap();
        let undo = svc.copying(&at, 2, None, None).await.unwrap().undo.unwrap();
        let places = crate::app::room::Places { own: vec![(2, "bob".into())], main: vec![0], incoming: vec![1] };
        let bobs = || crate::app::room::used_by(&c, &places, &std::collections::HashMap::new()).unwrap().get("bob").copied().unwrap_or(0);
        assert_eq!(lying(&own.join("A/Album")).len(), 10, "the ten copies");
        assert_eq!(bobs(), 0, "ten copies sharing Music's blocks: free");
        for n in [2, 5, 9] {
            std::fs::write(own.join(format!("A/Album/{n:02}.flac")), format!("track {n}, its cover fixed")).unwrap();
        }
        std::fs::write(own.join("A/Album/11.flac"), b"added since").unwrap();
        c.walk_all();
        let size = |n: &str| std::fs::metadata(own.join("A/Album").join(n)).unwrap().len();
        let its_own: u64 = ["02.flac", "05.flac", "09.flac", "11.flac"].iter().map(|n| size(n)).sum();
        assert_eq!(bobs(), its_own, "the rewritten three and the added file count; the seven still Music's bytes do not");
        let e = svc.undo(&undo).await.unwrap_err().to_string();
        assert!(e.contains("delete the copy"), "refused whole, saying so: {e}");
        assert_eq!(lying(&own.join("A/Album")).len(), 11, "all ten copies and the added file in place: no short album");
        assert!(lying(&own.join(crate::trash::TRASH_DIR)).is_empty(), "nothing in the trash");
        assert_eq!(bobs(), its_own, "his room as before the Undo");
        for d in [music, inc, own] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// AL01.6 (INTERACTIONS top 10, I22): ann's room holds her own music and her arrival in Incoming, asked for by ann, then by bob.
    /// Ann's Keep of it into her own music, through the service, leaves her room as it was (the same files, counted from her own
    /// music now, no longer as an arrival); bob's room never counts the arrival, before or after. Never one copy in two accounts'
    /// room, nor counted twice.
    #[tokio::test]
    async fn an_arrival_is_counted_once() {
        use crate::app::requests::{APPROVED, requesters_in};
        let (music, inc, ann, bob) = (folder("al01-once-music"), folder("al01-once-inc"), folder("al01-once-ann"), folder("al01-once-bob"));
        let svc = on_disk(&music, &inc).await;
        let c = svc.lib.catalogue();
        c.set_folders(vec![music.clone(), inc.clone(), ann.clone(), bob.clone()]).unwrap();
        std::fs::create_dir_all(ann.join("Mine")).unwrap();
        std::fs::write(ann.join("Mine/1.flac"), "her own, held before").unwrap();
        std::fs::create_dir_all(inc.join("B/Album")).unwrap();
        std::fs::write(inc.join("B/Album/1.flac"), "arrived, one").unwrap();
        std::fs::write(inc.join("B/Album/2.flac"), "arrived, two").unwrap();
        c.walk_all();
        c.pretend_read_for_tests();
        c.identify().unwrap();
        assert!(c.track_of(1, "B/Album/1.flac").is_some(), "the arrival's tracks");
        // Its record, as a build's record pass gives the arrival's tracks one.
        let record = c.record_for_tests("Album", 0);
        c.execute_for_tests(&format!("UPDATE tracks SET record = {record} WHERE id IN (SELECT track FROM files WHERE folder = 1)"));
        let ud = crate::userdata::UserData::open(None).unwrap();
        for (user, at) in [("ann", 1), ("bob", 2)] {
            let asked = crate::userdata::Request {
                id: 0,
                user: user.into(),
                album_key: format!("k-{user}"),
                artist_key: "b".into(),
                release_group: None,
                album: "Album".into(),
                artist: "B".into(),
                year: None,
                cover: None,
                at,
                status: APPROVED.into(),
                reason: None,
                decided_by: None,
                decided_at: None,
                fetched: None,
                record: Some(record),
            };
            ud.add_request(&asked).unwrap();
        }
        let kinds = [(0, settings::LibraryKind::Main), (1, settings::LibraryKind::Incoming), (2, settings::LibraryKind::Own), (3, settings::LibraryKind::Own)];
        let places = crate::app::room::Places { own: vec![(2, "ann".into()), (3, "bob".into())], main: vec![0], incoming: vec![1] };
        let rooms = || {
            let used = crate::app::room::used_by(&c, &places, &requesters_in(&c, &ud, &kinds)).unwrap();
            (used.get("ann").copied().unwrap_or(0), used.get("bob").copied().unwrap_or(0))
        };
        let size = |p: PathBuf| std::fs::metadata(p).unwrap().len();
        let held = size(ann.join("Mine/1.flac")) + size(inc.join("B/Album/1.flac")) + size(inc.join("B/Album/2.flac"));
        assert_eq!(rooms(), (held, 0), "her own music and her arrival; nothing of it bob's");
        let at = svc.locate_listed("cratarr", copy_at(&format!("al-{record}"), 1, &inc.to_string_lossy(), "B/Album")).await.unwrap();
        svc.move_located("cratarr", at, 2, settings::Quality::None, None).await.unwrap();
        assert!(!inc.join("B/Album/1.flac").exists(), "kept into her own music");
        assert_eq!(rooms(), (held, 0), "the same files, counted from her own music now: once");
        for d in [music, inc, ann, bob] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// A record of these files' tracks (catalogue folder `folder`, under `dir`), as a build's record pass gives an arrival's tracks
    /// one: its id.
    fn record_of_files(svc: &IncomingService, title: &str, folder: i64, dir: &str) -> i64 {
        let c = svc.lib.catalogue();
        let record = c.record_for_tests(title, 0);
        c.execute_for_tests(&format!("UPDATE tracks SET record = {record} WHERE id IN (SELECT track FROM files WHERE folder = {folder} AND rel LIKE '{dir}/%')"));
        record
    }

    /// `user`'s approved request of `record`, asked at `at`, answered by root at `at + 50`, kept in the service's user data.
    fn approved_of(svc: &IncomingService, user: &str, record: i64, at: i64) -> crate::userdata::Request {
        let r = crate::userdata::Request {
            id: 0,
            user: user.into(),
            album_key: format!("k-{user}-{record}"),
            artist_key: "a".into(),
            release_group: None,
            album: "Album".into(),
            artist: "A".into(),
            year: None,
            cover: None,
            at,
            status: crate::app::requests::APPROVED.into(),
            reason: None,
            decided_by: Some("root".into()),
            decided_at: Some(at + 50),
            fetched: Some(crate::app::requests::SOULSEEK_SEARCHES.into()),
            record: Some(record),
        };
        crate::userdata::Request { id: svc.lib.playlists_db().add_request(&r).unwrap(), ..r }
    }

    /// AL02.4 (Q1; the owner's "No approval per request"; the plan's `keep_over_room_is_refused`, replaced): adding is never refused
    /// for room. Ann at 99 of 100 GB, with her 3 GB arrival in Incoming and another of 0.5 GB (102.5 counted, over). Her Add to's
    /// sheet (the move's plan) has no refusal, and the Keep lands in her own music, her room as it was (the same files, counted once,
    /// from her own music now); Home Assistant's promote of her other arrival, a Keep naming no library, goes where the page's
    /// default does (her own music, its requester's) and lands; root's Copy to of Music's album into her own music while she is over
    /// is made, counted as R.0 has it (free where it shares Music's blocks). Never "over room" as a refusal of a Keep or of an
    /// administrator's Copy.
    #[tokio::test]
    async fn adding_is_never_refused_for_room() {
        use crate::app::requests::requesters_in;
        use crate::settings::{Libraries, LibrarySetting, SourceConfig};
        const GB: u64 = 1_000_000_000;
        let top = folder("al02-add");
        let (music, inc, own) = (top.join("music"), top.join("inc"), top.join("own"));
        let ann = own.join("ann");
        for d in [&music, &inc, &ann] {
            std::fs::create_dir_all(d).unwrap();
        }
        let svc = on_disk(&music, &inc).await;
        let c = svc.lib.catalogue();
        c.set_folders(vec![music.clone(), inc.clone(), ann.clone()]).unwrap();
        for (root, dir, files) in [(&music, "A/Album", 2), (&ann, "Mine", 1), (&inc, "B/Album", 2), (&inc, "C/Album", 1)] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
            for n in 1..=files {
                std::fs::write(root.join(dir).join(format!("{n}.flac")), format!("{dir} {n}")).unwrap();
            }
        }
        c.walk_all();
        c.pretend_read_for_tests();
        c.identify().unwrap();
        let (x, y) = (record_of_files(&svc, "Arrival", 1, "B/Album"), record_of_files(&svc, "Another Arrival", 1, "C/Album"));
        approved_of(&svc, "ann", x, 1);
        approved_of(&svc, "ann", y, 1);
        c.execute_for_tests(&format!("UPDATE files SET size = {} WHERE folder = 2", 99 * GB));
        c.execute_for_tests(&format!("UPDATE files SET size = {} WHERE folder = 1 AND rel LIKE 'B/%'", 3 * GB / 2));
        c.execute_for_tests(&format!("UPDATE files SET size = {} WHERE folder = 1 AND rel LIKE 'C/%'", GB / 2));
        let ud = svc.lib.playlists_db().clone();
        let kinds = [(0, settings::LibraryKind::Main), (1, settings::LibraryKind::Incoming), (2, settings::LibraryKind::Own)];
        let places = crate::app::room::Places { own: vec![(2, "ann".into())], main: vec![0], incoming: vec![1] };
        let room = || crate::app::room::used_by(&c, &places, &requesters_in(&c, &ud, &kinds)).unwrap().get("ann").copied().unwrap_or(0);
        let over = room();
        assert_eq!(over, 99 * GB + 3 * GB + GB / 2, "over her 100 GB");
        // Her Add to: its sheet, then the Keep.
        let at = svc.locate_listed("cratarr", copy_at(&crate::serve::ids::record(x), 1, &inc.to_string_lossy(), "B/Album")).await.unwrap();
        let (_, sheet) = svc.plan_move(&at, 2, None).await.unwrap();
        assert!(sheet.is_ok(), "the sheet has no refusal: {:?}", sheet.err());
        svc.move_located("cratarr", at, 2, settings::Quality::None, None).await.expect("her Keep, over room");
        assert!(ann.join("B/Album/1.flac").exists() && !inc.join("B/Album").exists(), "it lands in her own music");
        assert_eq!(room(), over, "counted once, from her own music now");
        // Home Assistant's promote of her other arrival: no library named, where the page's default goes.
        let placed = |p: &std::path::Path| crate::placement::PlacedLibrary {
            id: p.to_string_lossy().into_owned(),
            name: p.to_string_lossy().into_owned(),
            folders: vec![p.to_string_lossy().into_owned()],
            paths: Vec::new(),
            read_only: false,
        };
        let values = [(settings::OWN_MUSIC.to_string(), serde_json::json!(own.to_string_lossy())), (settings::OWN_MUSIC_OFF.to_string(), serde_json::json!([]))].into_iter().collect();
        let libraries = Libraries(vec![LibrarySetting::new(music.to_string_lossy().into_owned(), settings::LibraryKind::Main), LibrarySetting::new(inc.to_string_lossy().into_owned(), settings::LibraryKind::Incoming)]);
        let cfg = SourceConfig { id: "cratarr".into(), kind: crate::source::cratarr::KIND.id.into(), enabled: true, libraries, values };
        let placement = crate::placement::Placement { roots: Vec::new(), libraries: [&music, &inc, &ann].into_iter().map(|p| placed(p)).collect() };
        let a = crate::app::library::Active { cfg, source: None, placement, error: None };
        let other = copy_at(&crate::serve::ids::record(y), 1, &inc.to_string_lossy(), "C/Album");
        let asked = requesters_in(&c, &ud, &kinds);
        let requester = crate::app::albums::requester_of(&asked, &source::Album { added: Some(epoch_now()), ..other.clone() }).cloned();
        assert_eq!(requester.as_deref(), Some("ann"), "her request's arrival");
        let to = crate::app::albums::default_target(&a, other.library.as_deref(), requester.as_deref()).expect("a default target");
        let target = c.folders().id_of(std::path::Path::new(&to)).expect("her own music's folder");
        assert_eq!(target, 2, "the promote goes into her own music: {to}");
        let at = svc.locate_listed("cratarr", other).await.unwrap();
        svc.move_located("cratarr", at, target, settings::Quality::None, None).await.expect("the promote, over room");
        assert!(ann.join("C/Album/1.flac").exists(), "it lands in her own music");
        assert_eq!(room(), over, "counted once");
        // Root's Copy to of Music's album into her own music, while she is over.
        let at = svc.locate_listed("cratarr", copy_at("al-1", 0, &music.to_string_lossy(), "A/Album")).await.unwrap();
        let root = crate::serve::who::Who { user: "root".into(), admin: true, scope: crate::serve::who::Scope::Full, home: true };
        crate::serve::who::acting_as(root, svc.copying(&at, 2, None, None)).await.expect("root's copy, she over room");
        assert_eq!(std::fs::read(ann.join("A/Album/1.flac")).unwrap(), b"A/Album 1", "made");
        let shared = c.writer_for_tests().query_row("SELECT min(shared) FROM files WHERE folder = 2 AND rel LIKE 'A/%'", [], |r| r.get::<_, i64>(0)).unwrap();
        let copied: u64 = ["1.flac", "2.flac"].iter().map(|n| std::fs::metadata(music.join("A/Album").join(n)).unwrap().len()).sum();
        assert_eq!(room(), over + if shared == 1 { 0 } else { copied }, "counted as R.0 has it");
        std::fs::remove_dir_all(&top).ok();
    }

    /// AL02.8 (OQ-D1): a Keep that makes requests done for their askers (into Music: done for everyone) delivers them at once and
    /// names them on its entry; its Undo opens them again as they were (approved, by whom and when), the album back in Incoming.
    #[tokio::test]
    async fn a_keeps_undo_reopens_the_requests_it_delivered() {
        use crate::app::requests::{Closed, DELIVERED, DELIVERED_BY_KEEP};
        let (music, inc) = (folder("al02-undeliver-music"), folder("al02-undeliver-inc"));
        let svc = on_disk(&music, &inc).await;
        std::fs::create_dir_all(inc.join("A/Album")).unwrap();
        std::fs::write(inc.join("A/Album/1.flac"), b"waiting").unwrap();
        let c = svc.lib.catalogue();
        c.walk_all();
        c.pretend_read_for_tests();
        c.identify().unwrap();
        // Its tags, as Music's names want them.
        let tags = [("albumartist", "Arp"), ("album", "Zebra"), ("artist", "Arp"), ("title", "One"), ("tracknumber", "1"), ("date", "2020")];
        for (n, (name, value)) in tags.iter().enumerate() {
            c.execute_for_tests(&format!("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (1, 'A/Album/1.flac', {n}, '{name}', '{name}', '{value}')"));
        }
        let record = record_of_files(&svc, "Album", 1, "A/Album");
        let rs = [approved_of(&svc, "ann", record, 100), approved_of(&svc, "bob", record, 110)];
        let ud = svc.lib.playlists_db().clone();
        let before: Vec<_> = rs.iter().map(|r| answer(&ud, r.id)).collect();
        let at = svc.locate_listed("cratarr", copy_at(&crate::serve::ids::record(record), 1, &inc.to_string_lossy(), "A/Album")).await.unwrap();
        let kept = svc.move_located("cratarr", at, 0, settings::Quality::None, None).await.unwrap();
        assert_eq!(rs.iter().map(|r| answer(&ud, r.id).0).collect::<Vec<_>>(), [DELIVERED, DELIVERED], "delivered at once");
        let id: EventId = kept.undo.clone().unwrap().parse().unwrap();
        let named: Vec<Closed> = serde_json::from_value(svc.journal.result(id, DELIVERED_BY_KEEP).unwrap().expect("named on its entry")).unwrap();
        let mut ids: Vec<i64> = named.iter().map(|c| c.id).collect();
        ids.sort();
        assert_eq!(ids, [rs[0].id, rs[1].id]);
        svc.undo(&kept.undo.unwrap()).await.unwrap();
        assert!(inc.join("A/Album/1.flac").exists(), "the album back in Incoming");
        assert_eq!(rs.iter().map(|r| answer(&ud, r.id)).collect::<Vec<_>>(), before, "open again, as they were");
        for d in [music, inc] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// T1.2: the Undo of an old Copy to leaves a later album at the copy's place alone: Ann deleted the copy, a Keep of another
    /// edition landed there; the Undo is refused, the later album and its rows as they were, and its own Undo works after.
    #[tokio::test]
    async fn undo_of_an_old_copy_leaves_a_later_album_alone() {
        let (svc, music, inc, own) = with_own_music("t1-later").await;
        let at = svc.locate_listed("cratarr", copy_at("al-1", 0, &music.to_string_lossy(), "A/Album")).await.unwrap();
        let undo = svc.copying(&at, 2, None, None).await.unwrap().undo.unwrap();
        svc.delete_listed("cratarr", copy_at("al-1", 2, &own.to_string_lossy(), "A/Album")).await.unwrap();
        std::fs::create_dir_all(inc.join("A/Album")).unwrap();
        std::fs::write(inc.join("A/Album/1.flac"), b"another edition, one").unwrap();
        std::fs::write(inc.join("A/Album/2.flac"), b"another edition, two").unwrap();
        let c = svc.lib.catalogue();
        c.walk_all();
        c.pretend_read_for_tests();
        c.identify().unwrap();
        let edition = svc.locate_listed("cratarr", copy_at("al-2", 1, &inc.to_string_lossy(), "A/Album")).await.unwrap();
        let kept = svc.move_located("cratarr", edition, 2, settings::Quality::None, None).await.unwrap();
        let later = lying(&own.join("A/Album"));
        let tracks = (c.track_of(2, "A/Album/1.flac"), c.track_of(2, "A/Album/2.flac"));
        assert!(tracks.0.is_some(), "the later album's rows");
        assert!(svc.undo(&undo).await.is_err(), "none of the copied files is there");
        assert_eq!(lying(&own.join("A/Album")), later, "the later album as it was");
        assert_eq!((c.track_of(2, "A/Album/1.flac"), c.track_of(2, "A/Album/2.flac")), tracks, "its rows as they were");
        svc.undo(&kept.undo.unwrap()).await.unwrap();
        assert_eq!(std::fs::read(inc.join("A/Album/1.flac")).unwrap(), b"another edition, one", "its own Undo works after");
        for d in [music, inc, own] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// T1.3: an Undo cut short after its first file reached the trash (its second on the entry, the entry not undone) is finished
    /// by a second tap: the other file goes too, the first counts as done, one trash entry each, both rows trashed, undone once.
    #[tokio::test]
    async fn an_undo_of_a_copy_cut_short_is_finished_by_a_second_tap() {
        let (svc, music, inc, own) = with_own_music("t1-short").await;
        let at = svc.locate_listed("cratarr", copy_at("al-1", 0, &music.to_string_lossy(), "A/Album")).await.unwrap();
        let undo = svc.copying(&at, 2, None, None).await.unwrap().undo.unwrap();
        let (id, stamp): (EventId, i64) = (undo.parse().unwrap(), 1_700_000_000);
        svc.journal.done(id, UNCOPIED, stamp, &serde_json::json!({ "stamp": stamp, "tokens": [] })).unwrap();
        crate::trash::trash_file(&own, "A/Album/1.flac", stamp).unwrap();
        svc.undo(&undo).await.unwrap();
        assert!(!own.join("A/Album").exists(), "the other file went too");
        let trash = lying(&own.join(crate::trash::TRASH_DIR));
        assert_eq!(trash.into_keys().collect::<Vec<_>>(), [format!("{stamp}/A/Album/1.flac"), format!("{stamp}/A/Album/2.flac")], "one trash entry each");
        let c = svc.lib.catalogue();
        for f in ["1.flac", "2.flac"] {
            assert!(c.has_row(2, &format!("{}/{stamp}/A/Album/{f}", crate::trash::TRASH_DIR)), "{f}: its row trashed");
        }
        assert_eq!(uncopied(&svc, id).len(), 2);
        assert!(svc.undo(&undo).await.is_err(), "undone once");
        for d in [music, inc, own] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// T1.4: a Copy into CD folders, undone: the folders it leaves empty go, up to the library's root and never the root; the
    /// entry holds each file's trash token, and `trash::restore_file` by them brings each back byte for byte.
    #[tokio::test]
    async fn an_undone_copy_keeps_its_trash_tokens_and_leaves_no_empty_folder() {
        let (svc, music, inc, own) = with_own_music("t1-tokens").await;
        let c = svc.lib.catalogue();
        let copy = |n: &str, to: &str| TrackMove { track: c.track_of(0, &format!("A/Album/{n}")), from: (0, format!("A/Album/{n}")), to: (2, to.into()), was: None };
        let event = Event::AlbumCopied {
            source: "cratarr".into(),
            album_id: "al-1".into(),
            title: "Zebra".into(),
            from_dir: (0, "A/Album".into()),
            to_dir: (2, "Artist/Album".into()),
            copies: vec![copy("1.flac", "Artist/Album/CD1/1.flac"), copy("2.flac", "Artist/Album/CD2/2.flac")],
        };
        let underway = svc.intend(epoch_now(), &event).unwrap();
        let id = underway.id();
        svc.finish_copy(id, &event, None, None).await.unwrap();
        svc.journal.applied(id).unwrap();
        drop(underway);
        svc.undo(&id.to_string()).await.unwrap();
        assert!(!own.join("Artist").exists() && own.is_dir(), "pruned up to her root, never her root");
        let tokens = uncopied(&svc, id);
        assert_eq!(tokens.len(), 2, "each file's token kept");
        for t in &tokens {
            crate::trash::restore_file(&own, t).unwrap();
        }
        assert_eq!(std::fs::read(own.join("Artist/Album/CD1/1.flac")).unwrap(), b"one");
        assert_eq!(std::fs::read(own.join("Artist/Album/CD2/2.flac")).unwrap(), b"two");
        for d in [music, inc, own] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// RF5's shape: an album held only in Bob's own music (catalogue folder 3), two FLAC tagged as Music's template names them (the
    /// year 2019), read and on their tracks; Music (0), Incoming (1) and Ann's own music (2) hold nothing of it. The service, and the
    /// four folders.
    async fn with_bobs_alone(tag: &str) -> (IncomingService, PathBuf, PathBuf, PathBuf, PathBuf) {
        let roots: Vec<PathBuf> = ["music", "inc", "ann", "bob"].iter().map(|n| folder(&format!("{tag}-{n}"))).collect();
        let svc = on_disk(&roots[0], &roots[1]).await;
        let c = svc.lib.catalogue();
        c.set_folders(roots.clone()).unwrap();
        std::fs::create_dir_all(roots[3].join("A/Album")).unwrap();
        for n in 1..=2 {
            std::fs::write(roots[3].join(format!("A/Album/{n}.flac")), format!("bob's flac {n}")).unwrap();
        }
        c.walk_all();
        c.pretend_read_for_tests();
        for n in 1..=2usize {
            let number = n.to_string();
            let tags = [("albumartist", "A"), ("artist", "A"), ("album", "Album"), ("title", TITLES[n - 1]), ("tracknumber", number.as_str()), ("date", "2019")];
            c.tag_for_tests(3, &format!("A/Album/{n}.flac"), &tags);
        }
        c.identify().unwrap();
        let mut roots = roots.into_iter();
        let mut next = || roots.next().unwrap();
        (svc, next(), next(), next(), next())
    }

    /// RF5 (the review of runs 11 and 12, RA1): an old Copy's Undo once its original is gone is refused before anything moves,
    /// saying "The original is gone: this is the album's only copy." (1) An administrator copied Bob's own album into Music, then
    /// Bob deleted his copy (into his trash): Music's copy, its rows and its trash as they were, the entry not undone, no Undo
    /// begun on it; once Bob's Delete is undone (the original back), the Undo goes and Bob's album stays as it was. (2) A Want's copy
    /// of Bob's in Ann's own music, then Bob deleted his copy and his trash emptied: refused alike. (3) Bob left (his own music out
    /// of use): refused. (4) One track's original gone (Bob deleted 2.flac alone): refused whole, both copies in place. (5) Bob's
    /// 1.flac rewritten since (a tag written: other bytes and time): refused. Never a copy whose bytes no original holds taken into
    /// the trash by an Undo.
    #[tokio::test]
    async fn an_old_copys_undo_is_refused_once_its_original_is_gone() {
        const GONE: &str = "The original is gone: this is the album's only copy.";
        for case in 1..=5 {
            let (svc, music, inc, ann, bob) = with_bobs_alone(&format!("rf5-{case}")).await;
            let c = svc.lib.catalogue();
            let theirs = lying(&bob);
            let at = svc.locate_listed("cratarr", copy_at("al-1", 3, &bob.to_string_lossy(), "A/Album")).await.unwrap();
            let (to, root) = if case == 1 { (0, music.clone()) } else { (2, ann.clone()) };
            let done = svc.copying(&at, to, None, None).await.unwrap();
            let key = done.undo.clone().unwrap();
            let id: EventId = key.parse().unwrap();
            let copies: Vec<String> = files_under(&root.join(&done.folder)).into_iter().map(|f| format!("{}/{f}", done.folder)).collect();
            assert_eq!(copies.len(), 2, "{case}: both copied");
            let deleted = match case {
                1 | 2 => Some(svc.delete_listed("cratarr", copy_at("al-1", 3, &bob.to_string_lossy(), "A/Album")).await.unwrap().undo),
                3 => {
                    c.set_folders(vec![music.clone(), inc.clone(), ann.clone()]).unwrap();
                    assert_eq!(c.folders().id_of(&ann), Some(2), "Ann's own music as it was");
                    None
                }
                4 => {
                    let token = crate::trash::trash_file(&bob, "A/Album/2.flac", epoch_now()).unwrap();
                    c.mark_trashed(3, "A/Album/2.flac", &token).unwrap();
                    None
                }
                _ => {
                    std::fs::write(bob.join("A/Album/1.flac"), b"bob's flac 1, its tag written").unwrap();
                    c.walk_all();
                    None
                }
            };
            if case == 2 {
                crate::trash::purge(&bob, epoch_now() + 30 * 86_400, 14).unwrap();
                assert!(lying(&bob.join(crate::trash::TRASH_DIR)).is_empty(), "his trash emptied");
            }
            let rows = || copies.iter().map(|rel| row_at(&c, to, rel)).collect::<Vec<_>>();
            let (lies, held) = (lying(&root), rows());
            assert!(held.iter().all(|r| r.as_ref().is_some_and(|(track, trashed)| track.is_some() && trashed.is_none())), "{case}: on their tracks: {held:?}");
            let e = svc.undo(&key).await.unwrap_err().to_string();
            assert_eq!(e, GONE, "{case}: refused, saying why");
            assert_eq!(lying(&root), lies, "{case}: the copy in place, nothing in its library's trash");
            assert!(lying(&root.join(crate::trash::TRASH_DIR)).is_empty(), "{case}: nothing in the trash");
            assert_eq!(rows(), held, "{case}: its rows as they were");
            assert!(svc.journal.get(id).unwrap().is_some_and(|e| e.applied && !e.undone), "{case}: the entry not undone");
            assert!(svc.journal.result(id, UNCOPIED).unwrap().is_none(), "{case}: no Undo begun on it");
            if let (1, Some(deleted)) = (case, deleted) {
                svc.undo(&deleted).await.unwrap();
                assert_eq!(lying(&bob), theirs, "Bob's album back as it was");
                assert_eq!(svc.undo(&key).await.unwrap(), "Zebra", "the original back: the Undo goes");
                assert!(!music.join(&done.folder).exists(), "the copy into Music's trash");
                assert_eq!(lying(&bob), theirs, "Bob's untouched by it");
            }
            for d in [music, inc, ann, bob] {
                std::fs::remove_dir_all(d).ok();
            }
        }
    }

    /// An album waiting in Incoming (catalogue folder 1), record 1, and an approved request of it by each of `users`, asked before
    /// it came and answered by root: what a Delete of the copy closes (Y4).
    fn waiting_with_requests(svc: &IncomingService, inc: &std::path::Path, users: &[&str]) -> Vec<crate::userdata::Request> {
        std::fs::create_dir_all(inc.join("A/Album")).unwrap();
        std::fs::write(inc.join("A/Album/1.flac"), b"waiting").unwrap();
        let c = svc.lib.catalogue();
        c.walk_all();
        c.pretend_read_for_tests();
        c.identify().unwrap();
        let ud = svc.lib.playlists_db();
        let asked = |user: &&str| crate::userdata::Request {
            id: 0,
            user: user.to_string(),
            album_key: format!("k-{user}"),
            artist_key: "a".into(),
            release_group: None,
            album: "Album".into(),
            artist: "A".into(),
            year: None,
            cover: None,
            at: 100,
            status: crate::app::requests::APPROVED.into(),
            reason: None,
            decided_by: Some("root".into()),
            decided_at: Some(150),
            fetched: None,
            record: Some(1),
        };
        users.iter().map(asked).map(|r| crate::userdata::Request { id: ud.add_request(&r).unwrap(), ..r }).collect()
    }

    /// A request's answer as it stands: status, why, by whom, when.
    fn answer(ud: &crate::userdata::UserData, id: i64) -> (String, Option<String>, Option<String>, Option<i64>) {
        let r = ud.request(id).unwrap().unwrap();
        (r.status, r.reason, r.decided_by, r.decided_at)
    }

    /// Y4 (the review's finding 4): the Undo of a requester's Delete opens again the request the Delete closed, as it was
    /// (approved, by whom and when), before Lidarr fetches again; and only that: the other asker's, never closed, and one a Delete
    /// closed before, are as they were.
    #[tokio::test]
    async fn undoing_a_delete_reopens_the_requests_it_closed() {
        use crate::app::requests::{APPROVED, REJECTED, closed_by, reject};
        let (music, inc) = (folder("reopen-music"), folder("reopen-inc"));
        let svc = on_disk(&music, &inc).await;
        let rs = waiting_with_requests(&svc, &inc, &["ann", "bob", "carol"]);
        let ud = svc.lib.playlists_db().clone();
        assert!(ud.set_request_status(rs[2].id, APPROVED, REJECTED, Some("Rejected"), None).unwrap(), "Carol's closed by an earlier Delete");
        let before: Vec<_> = rs.iter().map(|r| answer(&ud, r.id)).collect();
        let deleted = svc.delete_listed("cratarr", copy_at("al-1", 1, &inc.to_string_lossy(), "A/Album")).await.unwrap();
        let undo = Some((svc.journal.as_ref(), deleted.undo.parse().unwrap()));
        let closed = reject(None, &ud, &closed_by(rs.clone(), false, Some("ann")), undo).await;
        assert_eq!(closed.iter().map(|c| c.id).collect::<Vec<_>>(), [rs[0].id], "Ann's alone");
        assert_eq!(answer(&ud, rs[0].id).0, REJECTED);
        svc.undo(&deleted.undo).await.unwrap();
        assert!(inc.join("A/Album/1.flac").exists(), "the album back");
        assert_eq!(rs.iter().map(|r| answer(&ud, r.id)).collect::<Vec<_>>(), before, "Ann's open again as it was; Bob's and Carol's as they were");
        for d in [music, inc] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// Y4: an administrator's Delete closes every asker's request, and its Undo opens them all again, each as it was.
    #[tokio::test]
    async fn an_administrators_delete_closes_every_request_and_its_undo_reopens_them() {
        use crate::app::requests::{REJECTED, closed_by, reject};
        let (music, inc) = (folder("reopen-all-music"), folder("reopen-all-inc"));
        let svc = on_disk(&music, &inc).await;
        let rs = waiting_with_requests(&svc, &inc, &["ann", "bob"]);
        let ud = svc.lib.playlists_db().clone();
        let before: Vec<_> = rs.iter().map(|r| answer(&ud, r.id)).collect();
        let deleted = svc.delete_listed("cratarr", copy_at("al-1", 1, &inc.to_string_lossy(), "A/Album")).await.unwrap();
        let undo = Some((svc.journal.as_ref(), deleted.undo.parse().unwrap()));
        assert_eq!(reject(None, &ud, &closed_by(rs.clone(), true, Some("root")), undo).await.len(), 2, "every asker's");
        assert!(rs.iter().all(|r| answer(&ud, r.id).0 == REJECTED));
        svc.undo(&deleted.undo).await.unwrap();
        assert_eq!(rs.iter().map(|r| answer(&ud, r.id)).collect::<Vec<_>>(), before, "all open again, each as it was");
        for d in [music, inc] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// A move finds what it moves by the copy too: with both folders named alike, the path's end would find Incoming's first
    /// whichever was asked, and "already in that library" would never be said.
    #[tokio::test]
    async fn move_cannot_hit_the_other_copy() {
        let (music, inc) = (folder("moved-music"), folder("moved-inc"));
        let svc = on_disk(&music, &inc).await;
        in_both(&music, &inc);
        let (kept, waiting) = (svc.locate_listed("cratarr", copy_at("al-1", 0, "/m", "A/Album")).await.unwrap(), svc.locate_listed("cratarr", copy_at("al-1", 1, "/in", "A/Album")).await.unwrap());
        assert_eq!((kept.dir.folder, kept.root.as_path()), (0, music.as_path()));
        assert_eq!((waiting.dir.folder, waiting.root.as_path()), (1, inc.as_path()));
        for d in [music, inc] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// An album over two artists' folders has the library's root for its `dir`: taken whole, that is the library. As not found.
    #[tokio::test]
    async fn a_copy_at_the_root_of_its_library_is_never_taken_whole() {
        let (music, inc) = (folder("root-music"), folder("root-inc"));
        let svc = on_disk(&music, &inc).await;
        in_both(&music, &inc);
        let refused = svc.delete_listed("cratarr", copy_at("al-1", 1, "/in", "")).await.err().unwrap();
        assert!(matches!(&refused, ToggleError::Refused(why) if why.contains("cannot see")), "{refused:?}");
        assert_eq!(std::fs::read(inc.join("A/Album/1.flac")).unwrap(), b"waiting");
        for d in [music, inc] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// Delete Music's copy and Incoming's is still waiting: by id it would vanish from Incoming's list and stay hidden for as
    /// long as that list has it.
    #[test]
    fn a_hidden_copy_does_not_hide_the_record() {
        let (music, waiting) = (copy_at("al-1", 0, "/m", "A/Album"), copy_at("al-1", 1, "/in", "A/Album"));
        let mut hidden = std::collections::HashSet::from([hidden_key(&music), hidden_key(&copy_at("al-2", 1, "/in", "B/Album"))]);
        assert!(!hidden.contains(&hidden_key(&waiting)), "the other copy shows");
        let listed = [hidden_key(&waiting), hidden_key(&copy_at("al-2", 1, "/in", "B/Album"))].into_iter().collect();
        still_hidden(&mut hidden, &listed);
        assert_eq!(hidden.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["al-2"], "gone from every list it was on, a copy needs no hiding; the one still listed does");
    }

    /// A copy of the record in another folder has an item of its own, and `source_id_of` takes any: a user's plays must reach the
    /// item at the place the file moved to.
    #[tokio::test]
    async fn play_data_goes_to_the_item_where_the_file_went() {
        let (music, inc) = (folder("items-music"), folder("items-inc"));
        let svc = on_disk(&music, &inc).await;
        in_both(&music, &inc);
        let c = svc.lib.catalogue();
        c.walk_all();
        c.pretend_read_for_tests();
        c.identify().unwrap();
        let rel = "A/Album/1.flac".to_string();
        c.record_links("jf", "m", &[("in-music".into(), 0, rel.clone())], 1).unwrap();
        c.record_links("jf", "in", &[("waiting".into(), 1, rel.clone())], 1).unwrap();
        let plan = |to: i64, old: &str| PlayData { source: "jf".into(), track: 1, old_item: old.into(), to: (to, rel.clone()), user: "u".into(), data: serde_json::Value::Null };
        assert_eq!(new_item(&c, &plan(0, "gone")).as_deref(), Some("in-music"));
        assert_eq!(new_item(&c, &plan(1, "gone")).as_deref(), Some("waiting"), "whichever a lookup by track would have found");
        assert_eq!(new_item(&c, &plan(1, "waiting")), None, "the old item: not linked anew yet");
        for d in [music, inc] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// Cratarr's own item is its track, so `new != old_item` was never true: a Keep of a played album stayed pending for the
    /// trash days. Its plan reads as nothing now, and is done with.
    #[tokio::test]
    async fn a_keep_of_cratarrs_own_leaves_no_play_data_waiting() {
        let (music, inc) = (folder("own-music"), folder("own-inc"));
        let svc = on_disk(&music, &inc).await;
        two_tracks(&inc);
        let id = svc.journal.intend(100, &keep_of_two()).unwrap();
        svc.journal.applied(id).unwrap();
        let before_this = serde_json::json!([{ "source": "cratarr", "track": 1, "old_item": "tr-1", "user": "ann", "data": {} }]);
        svc.journal.done(id, PLAYDATA_PLAN, 100, &before_this).unwrap();
        let entry = svc.journal.get(id).unwrap().unwrap();
        svc.carry_play_data(&entry).await;
        assert_eq!(svc.journal.result(id, PLAYDATA).unwrap(), Some(serde_json::json!(0)), "not left pending");
        for d in [music, inc] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// Two Incoming libraries can hold one record: Keep, Delete and Replace each land on one, so which is said or refused.
    #[test]
    fn a_record_waiting_in_two_libraries_must_be_told_which() {
        let listed = [copy_at("al-1", 1, "lidarr", "A/Album"), copy_at("al-1", 2, "soulseek", "A/Album"), copy_at("al-2", 1, "lidarr", "B/Album")];
        let named = |id: &str, from: Option<&str>| waiting_copy(listed.iter(), id, from).map(|a| a.copies[0].folder);
        assert_eq!(named("al-1", Some("soulseek")).unwrap(), 2);
        assert_eq!(named("al-1", Some("lidarr")).unwrap(), 1);
        assert!(matches!(named("al-1", None), Err(ToggleError::Refused(why)) if why.contains("which")));
        assert_eq!(named("al-2", None).unwrap(), 1, "one copy: nothing to tell");
        assert!(matches!(named("al-2", Some("soulseek")), Err(ToggleError::Refused(why)) if why.contains("no longer")), "not waiting there");
    }

    /// A server whose libraries in use are `libraries`, placed as `placement`.
    fn active(libraries: &[(&str, settings::LibraryKind)], placement: crate::placement::Placement) -> Active {
        let libraries = settings::Libraries(libraries.iter().map(|(id, k)| settings::LibrarySetting::new(*id, *k)).collect());
        let cfg = settings::SourceConfig { id: "jf".into(), kind: "jellyfin".into(), enabled: true, libraries, values: Default::default() };
        Active { cfg, source: None, placement, error: None }
    }

    #[test]
    fn a_folder_is_named_by_the_used_library_whose_files_lie_there() {
        let placed = |id: &str, folder: &str| crate::placement::PlacedLibrary { id: id.into(), name: id.into(), folders: vec![folder.into()], paths: Vec::new(), read_only: false };
        let placement = crate::placement::Placement { roots: Vec::new(), libraries: vec![placed("m", "/music"), placed("unused", "/incoming"), placed("i", "/incoming")] };
        let a = active(&[("m", settings::LibraryKind::Main), ("i", settings::LibraryKind::Incoming)], placement);
        let at = |folder: &str| library_in(&a, std::path::Path::new(folder));
        assert_eq!(at("/music").as_deref(), Some("m"));
        assert_eq!(at("/incoming").as_deref(), Some("i"), "a library not used is passed over");
        assert_eq!(at("/elsewhere"), None);
    }

    #[test]
    fn keep_follows_the_music_librarys_own_quality_limit() {
        let placement = crate::placement::Placement {
            roots: Vec::new(),
            libraries: vec![crate::placement::PlacedLibrary { id: "m".into(), name: "Music".into(), folders: vec!["/music".into()], paths: Vec::new(), read_only: false }],
        };
        let mut a = active(&[("m", settings::LibraryKind::Main)], placement);
        let music = std::path::Path::new("/music");
        assert_eq!(quality_in(&a, music), settings::Quality::Cd, "a main library's by its kind");
        a.cfg.libraries.0[0].quality = Some(settings::Quality::None);
        assert_eq!(quality_in(&a, music), settings::Quality::None, "set to none: hi-res files stay as they are");
        a.cfg.libraries.0[0].quality = Some(settings::Quality::Hr96);
        assert_eq!(quality_in(&a, music), settings::Quality::Hr96);
        assert_eq!(quality_in(&a, std::path::Path::new("/elsewhere")), settings::Quality::Cd);
    }

    #[test]
    fn what_a_move_converts_follows_the_targets_limit() {
        use crate::convert::tests::{sine, write_wav24};
        let root = std::env::temp_dir().join(format!("cratarr-to-convert-{}", std::process::id()));
        let album = root.join("A");
        std::fs::create_dir_all(&album).unwrap();
        write_wav24(&album.join("01 Hi.wav"), 192_000, &sine(192_000, 440.0, 0.3, 0.05, 2));
        write_wav24(&album.join("02 Mid.wav"), 96_000, &sine(96_000, 440.0, 0.3, 0.05, 2));
        write_wav24(&album.join("03 Low.wav"), 44_100, &sine(44_100, 440.0, 0.3, 0.05, 2));
        // Lossy: never converted, whatever the limit.
        let mut mp3 = vec![0xFF, 0xFB, 0x94, 0x64];
        mp3.resize(384 * 4, 0);
        std::fs::write(album.join("04 Lossy.mp3"), mp3).unwrap();
        let rels: Vec<String> = ["01 Hi.wav", "02 Mid.wav", "03 Low.wav", "04 Lossy.mp3"].map(String::from).into();
        let count = |q: settings::Quality| q.limit().map_or(0, |l| to_convert(&root, "A", &rels, l).len());
        assert_eq!(count(settings::Quality::Cd), 3, "24 bit at 44.1 kHz is above CD too");
        assert_eq!(count(settings::Quality::Hr48), 2);
        assert_eq!(count(settings::Quality::Hr96), 1);
        assert_eq!(count(settings::Quality::None), 0);
        std::fs::remove_dir_all(&root).ok();
    }

    /// The service of `on_disk` with Cratarr's own server in use: `music` its main library, `inc` its Incoming.
    async fn on_server(music: &std::path::Path, inc: &std::path::Path) -> IncomingService {
        let svc = on_disk(music, inc).await;
        let (m, i) = (music.to_string_lossy().into_owned(), inc.to_string_lossy().into_owned());
        let placed = |p: &str| crate::placement::PlacedLibrary { id: p.into(), name: p.into(), folders: vec![p.into()], paths: Vec::new(), read_only: false };
        let libraries = settings::Libraries(vec![settings::LibrarySetting::new(m.clone(), settings::LibraryKind::Main), settings::LibrarySetting::new(i.clone(), settings::LibraryKind::Incoming)]);
        let cfg = settings::SourceConfig { id: "cratarr".into(), kind: crate::source::cratarr::KIND.id.into(), enabled: true, libraries, values: Default::default() };
        let placement = crate::placement::Placement { roots: Vec::new(), libraries: vec![placed(&m), placed(&i)] };
        svc.lib.switch_to(vec![Arc::new(Active { cfg, source: None, placement, error: None })]).await.unwrap();
        svc
    }

    /// F33.1, F33.2: a Save to is told on Health's list of changes as "Saved to <library>: <album>", its kind still `copy`; Keep's
    /// "Added to", Move's "Moved:" and Replace's "Replaced in" as before.
    #[tokio::test]
    async fn a_save_to_is_told_as_saved() {
        let (music, inc) = (folder("f33-music"), folder("f33-inc"));
        let svc = on_server(&music, &inc).await;
        let m = music.to_string_lossy().into_owned();
        assert_eq!(svc.copied_named("cratarr", 0, "Tide Tables"), ("copy", format!("Saved to {m}: Tide Tables")));
        let src = include_str!("incoming.rs").replace("\r\n", "\n");
        for kept in ["(\"keep\", format!(\"Added to {}: {title}\"", "(\"move\", format!(\"Moved: {title}\")", "(\"replace\", format!(\"Replaced in {} from {}: {title}\""] {
            assert!(src.contains(kept), "{kept}");
        }
        assert!(!src.contains(&["\"Copied", " to {}: {title}\""].concat()), "no run written now says Copied to");
        for d in [music, inc] {
            std::fs::remove_dir_all(d).ok();
        }
    }

    /// F34.1, F34.3: a Move's and a Save to's target is never an Incoming library (its fetcher fills it alone): refused with the owner's
    /// words; a main library still a target. Each door asks at its start (the preview and the tap alike), so a library made Incoming
    /// after its sheet was shown is refused at the tap. Keep's own check, and Lidarr's and Soulseek's placing, as before.
    #[tokio::test]
    async fn incoming_is_no_destination() {
        let (music, inc) = (folder("f34-music"), folder("f34-inc"));
        let svc = on_server(&music, &inc).await;
        let (m, i) = (music.to_string_lossy().into_owned(), inc.to_string_lossy().into_owned());
        assert!(matches!(svc.destination_of("cratarr", &i), Err(ToggleError::Refused(why)) if why == INCOMING_IS_NO_DESTINATION));
        assert_eq!(svc.destination_of("cratarr", &m).ok().map(|(f, _)| f), Some(0), "a main library still a target");
        let src = include_str!("incoming.rs").replace("\r\n", "\n");
        for door in ["pub async fn move_album(", "pub async fn move_preview(", "pub async fn copy_album(", "pub async fn copy_preview("] {
            let at = src.find(door).unwrap_or_else(|| panic!("{door}"));
            let body = &src[at..at + src[at..].find("\n    }\n").unwrap()];
            assert!(body.contains("self.destination_of(source_id, to)?") && !body.contains("self.target_of("), "{door}");
        }
        for d in [music, inc] {
            std::fs::remove_dir_all(d).ok();
        }
    }
}

/// Where a record's copy in catalogue folder `folder` lies. Cratarr's own alone: another server's id is no record.
pub(super) async fn copy_dir_in(album: &str, folder: i64) -> Option<String> {
    let (served, id) = (crate::serve::instance()?, album.to_string());
    let copies = crate::run::spawn_blocking(move || served.library.copies_of(&id)).await.ok()?.ok()?;
    copies.into_iter().find(|c| c.folder == folder).map(|c| c.dir)
}

/// What became of an album that left a library (`IncomingService::album_fate`). Asked only of an album no longer found, so
/// `Deleted` holds though a record's other copies could have stayed: none did. Moved is another server's: its files get new
/// items and the album a new id, where a record of Cratarr's own keeps its id and is only not read yet.
async fn fate_of(lib: &Library, journal: &Journal, source_id: &str, album_id: &str) -> Option<AlbumFate> {
    let event = journal.album_fate(source_id, album_id).ok()??;
    let (moves, title) = match &event {
        Event::AlbumAdopted { moves, title, .. } | Event::AlbumReplaced { moves, title, .. } => (moves, title),
        Event::AlbumRejected { title, .. } => return Some(AlbumFate::Deleted(title.clone())),
        Event::AlbumCopied { .. } | Event::AlbumReplacedFromCopy { .. } => return None,
    };
    if source_id == crate::source::cratarr::KIND.id {
        return Some(AlbumFate::NotYet(title.clone()));
    }
    let s = lib.connected_sources().into_iter().find(|(id, _)| id == source_id).map(|(_, s)| s)?;
    // Its tracks' new items (once a sync pass linked them), and the album the server put them in.
    let Some(folder) = moves.first().map(|m| m.to.0) else { return Some(AlbumFate::NotYet(title.clone())) };
    let rels: Vec<String> = moves.iter().filter(|m| m.track.is_some()).map(|m| m.to.1.clone()).collect();
    let items = lib.catalogue().track_ids(source_id, folder, &rels).unwrap_or_default();
    for item in items.into_iter().flatten().take(3) {
        if let Ok(t) = s.track(&item).await
            && let Some(album) = t.album_id
        {
            return Some(AlbumFate::Moved(album));
        }
    }
    Some(AlbumFate::NotYet(title.clone()))
}
