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
mod queue;
mod replace;
pub use queue::JobView;
pub use replace::ReplacePreview;

/// The handler name in the journal for telling Lidarr (decision 0011 §6). Its result is what it stopped fetching.
const ACQUISITION: &str = "acquisition";
/// What the acquisition handler is about to stop fetching, recorded before it acts (plan, then act).
const ACQUISITION_PLAN: &str = "acquisition-plan";
/// The handler that gives moved files' new server items the play data of their old ones (decision 0013).
const PLAYDATA: &str = "playdata";
/// What each moved file's old item held (plays, favourites…), read before the move: the server forgets it after.
const PLAYDATA_PLAN: &str = "playdata-plan";

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
    /// Whether a Lidarr is set, to be told to stop fetching the album; without one it may fetch it again.
    pub acquisition: bool,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum KeepRefused {
    NeedsYear,
    /// Another reason, said as is.
    Other(String),
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
fn files_under(dir: &std::path::Path) -> Vec<String> {
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
    /// Per album id, when the album is already in another used library: an upgrade of it or a duplicate (new albums
    /// are absent).
    pub verdicts: BTreeMap<String, VerdictView>,
    /// The albums Cratarr can delete: it sees their folder and can write there.
    pub deletable: Vec<String>,
    /// Per album id, what it lacks of what the library requires (the owner, 2026-10-24: marked before Keep).
    pub lacks: BTreeMap<String, Vec<&'static str>>,
    /// The libraries it was read from (the filter's library switches).
    pub libraries: Vec<String>,
}

/// How an incoming album relates to the other used libraries (decision 0034).
#[derive(Debug, Clone, Serialize)]
pub struct VerdictView {
    /// "upgrade" or "duplicate".
    pub kind: &'static str,
    /// The album it matches (its folder).
    pub library: String,
    pub why: String,
    /// The library that album is in (its id, as `library_info` has it); none when no used library of the server lies
    /// in its folder.
    pub library_id: Option<String>,
    /// The catalogue folder of the main library that album is in: Replace puts this one in its place there (0101 point
    /// 2); none for another library's, only told.
    #[serde(skip)]
    pub main_folder: Option<i64>,
    /// The main libraries Replace can put this one into, in its other copy's place (0101 point 2).
    pub replaceable_into: Vec<String>,
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

/// Lidarr on the Settings page. The key is never sent.
#[derive(Debug, Clone, Serialize)]
pub struct LidarrView {
    pub url: String,
    pub key_set: bool,
    /// Switched on; off, not asked.
    pub enabled: bool,
    /// What Discover's Want adds an album with (`settings::LidarrConfig`); none: as the artists Lidarr has.
    pub quality_profile: Option<i64>,
    pub metadata_profile: Option<i64>,
    pub root_folder: Option<String>,
    /// Lidarr's version when it answered.
    pub version: Option<String>,
    pub error: Option<String>,
}

/// Each album already in another used library, by album id: an upgrade of it or a duplicate. Against every other
/// library (decision 0034), the main libraries first, never its own; one whose folder Cratarr cannot see, against the
/// main libraries alone (in its own library it would be found as itself). `libraries`: each library's index by
/// catalogue folder's id, `main` the main libraries' ids (0101 point 1); `library_in`: the library id of a folder. The
/// album's release group comes from the catalogue, matched by the end of its folder path (the server's paths and
/// Cratarr's differ in front).
fn verdicts(
    albums: &[source::Album],
    libraries: &[(i64, crate::incoming::LibraryIndex)],
    main: &[i64],
    incoming: &[IncomingDir],
    library_in: impl Fn(i64) -> Option<String>,
) -> BTreeMap<String, VerdictView> {
    use crate::incoming::{AlbumFacts, Verdict, quality_of};
    // The main libraries first, then the others, each in its id's order.
    let mut order: Vec<&(i64, crate::incoming::LibraryIndex)> = libraries.iter().collect();
    order.sort_by_key(|(f, _)| (!main.contains(f), *f));
    let mut out = BTreeMap::new();
    for a in albums {
        // Cratarr's own says where it lies: by the path's end alone, two libraries' folders of one name are one.
        let own = match a.copies.first() {
            Some(c) => incoming.iter().find(|d| d.folder == c.folder && d.dir == c.dir),
            None => a.path.as_deref().and_then(|p| incoming_dir(incoming, p)),
        };
        let facts = AlbumFacts {
            release_group: own.and_then(|d| d.release_group.clone()),
            artist: a.artist.clone().unwrap_or_default(),
            title: a.title.clone(),
            tracks: a.tracks,
            quality: quality_of(a.format.as_deref(), a.bitrate_kbps),
        };
        let mut others = order.iter().map(|(f, index)| (*f, index)).filter(|(f, _)| own.map_or(main.contains(f), |d| *f != d.folder));
        let found = others.find_map(|(f, index)| match index.classify(&facts) {
            Verdict::New => None,
            Verdict::Upgrade { library, why } => Some((f, "upgrade", index.albums[library].0.clone(), why)),
            Verdict::Duplicate { library, why } => Some((f, "duplicate", index.albums[library].0.clone(), why)),
        });
        let Some((folder, kind, library, why)) = found else { continue };
        let library_id = library_in(folder);
        let main_folder = main.contains(&folder).then_some(folder);
        let replaceable_into = if main_folder.is_some() { library_id.clone().into_iter().collect() } else { Vec::new() };
        out.insert(a.id.clone(), VerdictView { kind, library, why, library_id, main_folder, replaceable_into });
    }
    out
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

/// Whether what lies under a copy's `dir` is that copy alone, as Delete and Move take the folder whole. `dir` is what its
/// files share, so it can be a parent of other records' folders, or the library's root: an album in two artists' folders has
/// the root for its `dir` (files A33). Blocking.
fn whole_copy(album: &str, folder: i64, root: &std::path::Path, dir: &str) -> Result<bool> {
    if dir.is_empty() {
        return Ok(false);
    }
    let Some(served) = crate::serve::instance() else { return Ok(true) };
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
    libraries: Arc<Vec<(i64, crate::incoming::LibraryIndex)>>,
    main: Vec<i64>,
    incoming: Arc<Vec<IncomingDir>>,
}

/// An album directory under a catalogue folder, as the catalogue knows it.
#[derive(Debug, Clone)]
struct IncomingDir {
    /// Catalogue folder id.
    folder: i64,
    dir: String,
    release_group: Option<String>,
}

/// A Lidarr connection and its client (`None` when the key cannot be used in a header).
pub(super) type LidarrPair = (Option<settings::LidarrConfig>, Option<crate::lidarr::Lidarr>);

pub(super) fn lidarr_pair(cfg: Option<settings::LidarrConfig>) -> LidarrPair {
    // Switched off: kept, not used.
    let client = cfg.as_ref().filter(|c| c.enabled).and_then(|c| match crate::lidarr::Lidarr::new(&c.url, &c.api_key) {
        Ok(l) => Some(l),
        Err(e) => {
            tracing::warn!("Lidarr: {e:#}");
            None
        }
    });
    (cfg, client)
}

/// Album actions going on in this process, by their journal ids.
type Running = Arc<std::sync::Mutex<std::collections::HashSet<EventId>>>;

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
    pub(super) fn new(
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
    pub(super) fn lidarr_state(&self) -> LidarrPair {
        self.lidarr.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Lidarr as the page sees it: its address, whether a key is saved, and whether it answers.
    pub(super) async fn lidarr_view(cfg: Option<&settings::LidarrConfig>, client: Option<crate::lidarr::Lidarr>) -> Option<LidarrView> {
        let cfg = cfg?;
        let (version, error) = match client {
            _ if !cfg.enabled => (None, None),
            Some(l) => match l.version().await {
                Ok(v) => (Some(v), None),
                Err(e) => (None, Some(format!("{e:#}"))),
            },
            None => (None, Some("the API key is not usable".into())),
        };
        Some(LidarrView {
            url: cfg.url.clone(),
            key_set: !cfg.api_key.is_empty(),
            enabled: cfg.enabled,
            quality_profile: cfg.quality_profile,
            metadata_profile: cfg.metadata_profile,
            root_folder: cfg.root_folder.clone(),
            version,
            error,
        })
    }

    pub(super) fn trash_days(&self) -> u32 {
        self.trash_days.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Take saved settings into use: the Lidarr connection and the trash days.
    pub(super) fn apply_settings(&self, lidarr: LidarrPair, trash_days: u32) {
        *self.lidarr.write().unwrap_or_else(|p| p.into_inner()) = lidarr;
        self.trash_days.store(trash_days, std::sync::atomic::Ordering::Relaxed);
    }

    /// Servers and libraries may have changed: the lists are read again on the next look (or the next sync pass).
    pub(super) async fn forget_lists(&self) {
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
                tokio::task::spawn_blocking(move || lib.lacking())
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
        if let Some((libraries, main, incoming)) = self.owned_index().await {
            let (folders, live) = (self.lib.catalogue().folders(), self.lib.live());
            for view in &mut sources {
                // Each pair's library by this server's ids (the page names it so).
                let a = live.iter().find(|a| a.cfg.id == view.id);
                let library_of = |f: i64| a.and_then(|a| library_in(a, folders.get(f)?));
                view.verdicts = verdicts(&view.albums, &libraries, &main, &incoming, library_of);
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
        let ok = tokio::task::spawn_blocking(move || files.can_write(&f)).await.unwrap_or(false);
        self.writable.lock().await.insert(folder.to_path_buf(), (now, ok));
        ok
    }

    /// Each library's index with its catalogue folder's id, the main libraries' folders, and the other folders' albums
    /// with their release groups, read from the catalogue at most every `INDEX_SECS`.
    async fn owned_index(&self) -> Option<(Arc<Vec<(i64, crate::incoming::LibraryIndex)>>, Vec<i64>, Arc<Vec<IncomingDir>>)> {
        let mut owned = self.owned.lock().await;
        if let Some(o) = owned.as_ref().filter(|o| epoch_now() - o.built < INDEX_SECS) {
            return Some((o.libraries.clone(), o.main.clone(), o.incoming.clone()));
        }
        let (c, main) = (self.lib.catalogue(), self.lib.main_folders());
        let main_ = main.clone();
        let read = tokio::task::spawn_blocking(move || -> Result<(Vec<(i64, crate::incoming::LibraryIndex)>, Vec<IncomingDir>)> {
            use crate::incoming::{AlbumFacts, LibraryIndex, quality_of};
            let (mut libraries, mut incoming) = (Vec::new(), Vec::new());
            let ids: Vec<i64> = c.folders().ids().collect();
            for id in ids {
                let found = c.album_facts(id)?;
                if !main_.contains(&id) {
                    incoming.extend(found.iter().map(|f| IncomingDir { folder: id, dir: f.dir.clone(), release_group: f.release_group.clone() }));
                }
                let library = found
                    .into_iter()
                    .map(|f| {
                        let facts = AlbumFacts {
                            release_group: f.release_group.clone(),
                            artist: f.album_artist.clone().or(f.artist.clone()).unwrap_or_default(),
                            title: f.album.clone().unwrap_or_else(|| f.dir.rsplit('/').next().unwrap_or("").to_string()),
                            tracks: f.tracks,
                            quality: quality_of(f.formats.iter().map(String::as_str), f.min_kbps),
                        };
                        (f.dir, facts)
                    })
                    .collect();
                libraries.push((id, LibraryIndex::new(library)));
            }
            Ok((libraries, incoming))
        })
        .await;
        match read {
            Ok(Ok((libraries, incoming))) => {
                let o = Owned { built: epoch_now(), libraries: Arc::new(libraries), main, incoming: Arc::new(incoming) };
                let out = (o.libraries.clone(), o.main.clone(), o.incoming.clone());
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
        let (tracks, leaving): (Vec<i64>, Vec<(i64, String)>) = tokio::task::spawn_blocking(move || {
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
        let trashed = tokio::task::spawn_blocking(move || {
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
        if let Err(e) = tokio::task::spawn_blocking(move || c.mark_trashed(f, &rel, &tok)).await.map_err(|e| anyhow::anyhow!("{e}")).and_then(|r| r) {
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
            };
            let (title, _) = entry.event.album();
            if happened && matches!(entry.event, Event::AlbumAdopted { .. } | Event::AlbumReplaced { .. }) {
                let moving = self.lib.catalogue().copies_moving();
                if let Some(steps) = self.steps(&entry.event) {
                    let (files, held) = (self.files.clone(), moving.clone());
                    let done = tokio::task::spawn_blocking(move || {
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
                    let _ = tokio::task::spawn_blocking(move || files.clear_parts(&steps)).await;
                }
                tracing::info!("forgot an action on {title} that never happened");
                self.journal.forget(entry.id)
            };
            if let Err(e) = result {
                tracing::warn!("journal: {e:#}");
            }
        }
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
        tokio::task::spawn_blocking(move || files.restore(&root, &t))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
        let (c, f, rel, tok) = (catalogue.clone(), *folder, dir.clone(), token.clone());
        if let Err(e) = tokio::task::spawn_blocking(move || c.unmark_trashed(f, &rel, &tok)).await.map_err(|e| anyhow::anyhow!("{e}")).and_then(|r| r) {
            tracing::warn!("catalogue: could not bring {title} back from the trash: {e:#}");
        }
        self.journal.mark_undone(id, epoch_now())?;
        self.resume_fetching(id, title).await;
        self.unhide(source, album_id, *folder).await;
        catalogue.poke();
        Ok(title.clone())
    }

    /// Find a listed album on disk: of Cratarr's own the copy it names, where it says it lies; another server's by its path.
    async fn locate_listed(&self, source_id: &str, album: source::Album) -> Result<Located, ToggleError> {
        let Some((folder, dir)) = album.copies.first().map(|c| (c.folder, c.dir.clone())) else { return self.locate_by_path(source_id, album).await };
        let root = self.root_of(folder)?;
        let (c, id, at, d) = (self.lib.catalogue(), album.id.clone(), root.clone(), dir.clone());
        let found = tokio::task::spawn_blocking(move || -> Result<Option<Option<String>>> { Ok(whole_copy(&id, folder, &at, &d)?.then(|| release_group_of(&c, folder, &d))) })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))??;
        let release_group = found.ok_or_else(|| ToggleError::Refused("Cratarr cannot see that album's folder".into()))?;
        Ok(Located { source: source_id.to_string(), album, dir: IncomingDir { folder, dir, release_group }, root })
    }

    /// Find another server's album on disk by the end of its path, in its own library's folder where the listing says
    /// which: two libraries can each hold a folder of that name.
    async fn locate_by_path(&self, source_id: &str, album: source::Album) -> Result<Located, ToggleError> {
        let path = album.path.clone().ok_or_else(|| ToggleError::Refused("the server gives no folder for that album".into()))?;
        let (libraries, main, incoming) = self.owned_index().await.ok_or_else(|| ToggleError::Refused("the catalogue is not ready yet".into()))?;
        let own = album.library.as_deref().and_then(|l| {
            let live = self.lib.live();
            self.folder_of(live.iter().find(|a| a.cfg.id == source_id)?, l).ok()
        });
        let incoming: Vec<IncomingDir> = incoming.iter().filter(|d| own.is_none_or(|o| o == d.folder)).cloned().collect();
        let in_main: Vec<IncomingDir> = libraries
            .iter()
            .filter(|(f, _)| main.contains(f) && own.is_none_or(|o| o == *f))
            .flat_map(|(f, m)| m.albums.iter().map(move |(dir, facts)| IncomingDir { folder: *f, dir: dir.clone(), release_group: facts.release_group.clone() }))
            .collect();
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

    /// Work out Keep for an album into the main library in catalogue folder `target`: its new names, or why not. Reads
    /// the disk and the catalogue; changes nothing.
    async fn plan_keep(&self, at: &Located, target: i64, year: Option<&str>) -> Result<(PathBuf, Result<crate::incoming::KeepPlan, KeepRefused>), ToggleError> {
        let music = self.root_of(target)?;
        let name = self.library_name(&at.source, target);
        let (c, folder, dir, root, music_, year, template) =
            (self.lib.catalogue(), at.dir.folder, at.dir.dir.clone(), at.root.clone(), music.clone(), year.map(str::to_string), self.lib.template());
        let plan = tokio::task::spawn_blocking(move || -> Result<Result<crate::incoming::KeepPlan, KeepRefused>> {
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
        Ok(match plan {
            Ok(p) => KeepPreview { title: at.album.title, folder: Some(p.folder), files: p.moves.len(), refused: None, convert, acquisition },
            Err(r) => KeepPreview { title: at.album.title, folder: None, files: 0, refused: Some(r), convert, acquisition },
        })
    }

    /// The album's music files that need converting first, under the target's quality limit (none: no file).
    async fn to_convert(&self, at: &Located, quality: settings::Quality) -> Vec<String> {
        let Some(limit) = quality.limit() else { return Vec::new() };
        let (c, folder, dir, root) = (self.lib.catalogue(), at.dir.folder, at.dir.dir.clone(), at.root.clone());
        tokio::task::spawn_blocking(move || {
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
        let (target, quality) = self.keep_target(source_id, to)?;
        self.move_located(source_id, at, target, quality, year).await
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

    /// The id of a server's used library in catalogue folder `folder`.
    fn library_at(&self, source_id: &str, folder: i64) -> Option<String> {
        let (live, folders) = (self.lib.live(), self.lib.catalogue().folders());
        library_in(live.iter().find(|a| a.cfg.id == source_id)?, folders.get(folder)?)
    }

    /// What a server calls its used library in catalogue folder `folder`, for what Keep, Move and Replace say; else
    /// "that library".
    fn library_name(&self, source_id: &str, folder: i64) -> String {
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
        let (target, quality) = self.target_of(source_id, to)?;
        if target == at.dir.folder {
            return Err(ToggleError::Refused("the album is in that library already".into()));
        }
        self.move_located(source_id, at, target, quality, year).await
    }

    /// What a move would do (decision 0034), for the page to show before asking. Changes nothing.
    pub async fn move_preview(&self, source_id: &str, album: source::Album, to: &str, year: Option<&str>) -> Result<KeepPreview, ToggleError> {
        let at = self.locate_listed(source_id, album).await?;
        let (target, quality) = self.target_of(source_id, to)?;
        let (_, plan) = self.plan_move(&at, target, year).await?;
        let convert = self.to_convert(&at, quality).await.len();
        let acquisition = self.acquisition().is_some() && !self.lib.is_kept(at.dir.folder);
        Ok(match plan {
            Ok(p) => KeepPreview { title: at.album.title, folder: Some(p.folder), files: p.moves.len(), refused: None, convert, acquisition },
            Err(r) => KeepPreview { title: at.album.title, folder: None, files: 0, refused: Some(r), convert, acquisition },
        })
    }

    async fn move_located(&self, source_id: &str, at: Located, target: i64, quality: settings::Quality, year: Option<&str>) -> Result<KeepResult, ToggleError> {
        let (root, plan) = self.plan_move(&at, target, year).await?;
        let plan = plan.map_err(|r| match r {
            KeepRefused::NeedsYear => ToggleError::Refused("the album's year is not known".into()),
            KeepRefused::Other(why) => ToggleError::Refused(why),
        })?;
        if !self.can_write(&at.root).await || !self.can_write(&root).await {
            return Err(ToggleError::Refused("Cratarr cannot write in both libraries".into()));
        }
        let convert = self.to_convert(&at, quality).await;
        if let (false, Some(limit)) = (convert.is_empty(), quality.limit()) {
            return Ok(self.enqueue(source_id, at, queue::Action::Keep, year, convert, limit, plan.folder, target));
        }
        self.keep_now(&at, target, year, Vec::new(), None).await
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
        let plan = tokio::task::spawn_blocking(move || {
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
        let run = super::file_runs::track(what, title);
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
        self.complete_covers(at).await;
        let (root, plan) = self.plan_move(at, target, year).await?;
        let plan = plan.map_err(|r| match r {
            KeepRefused::NeedsYear => ToggleError::Refused("the album's year is not known".into()),
            KeepRefused::Other(why) => ToggleError::Refused(why),
        })?;
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
        let moved = tokio::task::spawn_blocking(move || {
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
        self.hidden.lock().await.insert(hidden_key(&at.album));
        *self.owned.lock().await = None;
        c.poke();
        if let Some(p) = playing {
            carry_on::carry_on(self.lib.clone(), p, &event);
        }
        Ok(KeepResult { undo: Some(id.to_string()), title: at.album.title.clone(), folder: plan.folder, queued: false })
    }

    /// Before an album moves into the library: its cover inside every track and as a file next to them (the owner,
    /// 2026-10-19; `covers_inside`), in its own folder, so the cover file moves with it and an Undo takes it back.
    /// Never stops the move: what could not be done is logged.
    async fn complete_covers(&self, at: &Located) {
        let dir = at.root.join(&at.dir.dir);
        let shown = dir.display().to_string();
        match tokio::task::spawn_blocking(move || crate::covers_inside::complete(&dir)).await {
            Ok(Ok(done)) => {
                if done.cover_file.is_some() || !done.embedded.is_empty() {
                    tracing::info!("covers: {shown}: cover file written: {}, inside {} tracks", done.cover_file.is_some(), done.embedded.len());
                }
                for (t, why) in done.left {
                    tracing::warn!("covers: {} left without a picture inside: {why}", t.display());
                }
            }
            Ok(Err(e)) => tracing::warn!("covers: {shown}: {e:#}"),
            Err(e) => tracing::warn!("covers: {shown}: {e}"),
        }
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
        let done = tokio::task::spawn_blocking(move || -> Result<()> {
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
        let moved = tokio::task::spawn_blocking(move || -> Result<()> {
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
        tokio::task::spawn_blocking(move || {
            let _held = held;
            files.move_all(&steps)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
        self.finish_adopt(&entry.event, true, false).await;
        self.unconvert_rows(&entry.event).await;
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
        // Undone while Lidarr was being told: put it back at once.
        if self.journal.get(entry.id).ok().flatten().is_some_and(|e| e.undone) {
            self.resume_fetching(entry.id, title).await;
        }
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
    pub(super) async fn catch_up(&self) {
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
    pub(super) fn listen(self: &Arc<Self>) {
        let (me, mut rx) = (Arc::downgrade(self), self.journal.subscribe());
        tokio::spawn(async move {
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
    pub(super) async fn purge_trash(&self) {
        let folders = self.lib.catalogue().folders();
        let (now, keep, files) = (epoch_now(), self.trash_days(), self.files.clone());
        let gone = tokio::task::spawn_blocking(move || {
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
            verdicts: BTreeMap::new(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::incoming::{AlbumFacts, LibraryIndex, Quality};

    fn album(id: &str, path: &str, format: &str) -> source::Album {
        source::Album {
            id: id.into(),
            title: "Zebra".into(),
            artist: Some("Arp".into()),
            year: None,
            added: None,
            path: Some(path.into()),
            tracks: 9,
            format: Some(format.into()),
            bitrate_kbps: Some(320),
            duration_secs: None,
            tags: Vec::new(),
            library: None,
            copies: Vec::new(),
            artists: Vec::new(),
            release: Default::default(),
        }
    }

    fn owned(dir: &str, quality: Quality) -> (String, AlbumFacts) {
        (dir.into(), AlbumFacts { artist: "Arp".into(), title: "Zebra".into(), tracks: 9, quality, ..AlbumFacts::default() })
    }

    fn dir(folder: i64, dir: &str) -> IncomingDir {
        IncomingDir { folder, dir: dir.into(), release_group: None }
    }

    /// The service on a main library `music` (catalogue folder 0) and an Incoming `inc` (1), no server, on the disk.
    pub(super) async fn on_disk(music: &std::path::Path, inc: &std::path::Path) -> IncomingService {
        std::fs::create_dir_all(music.join(crate::playlists::PLAYLIST_DIR)).unwrap();
        let db = Arc::new(crate::userdata::UserData::open(None).unwrap());
        let lib = Library::new(None, Some(music.to_string_lossy().into_owned()), Vec::new(), Default::default(), None, db).await.unwrap();
        lib.catalogue().set_folders(vec![music.to_path_buf(), inc.to_path_buf()]).unwrap();
        let shelves = Arc::new(Shelves::new(lib.clone(), None));
        IncomingService::new(lib, crate::ports::disk(), Arc::new(Journal::open(None).unwrap()), None, crate::trash::DEFAULT_KEEP_DAYS, None, shelves)
    }

    fn folder(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cratarr-incoming-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
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
    fn copy_at(id: &str, folder: i64, library: &str, dir: &str) -> source::Album {
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

    #[test]
    fn an_incoming_album_is_judged_against_every_other_library_but_its_own() {
        // Music lacks it; one Incoming library has it at 320 kbps, another lossless.
        let incoming = [dir(1, "Arp/Zebra"), dir(2, "Arp - Zebra")];
        let albums = [album("i1", "/data/lidarr/Arp/Zebra", "MP3"), album("i2", "/data/new/Arp - Zebra", "FLAC"), album("i3", "/elsewhere/Zebra", "FLAC")];
        let libraries = [(0, LibraryIndex::new(Vec::new())), (1, LibraryIndex::new(vec![owned("Arp/Zebra", Quality::Lossy { kbps: 320 })])), (2, LibraryIndex::new(vec![owned("Arp - Zebra", Quality::Lossless)]))];
        let v = verdicts(&albums, &libraries, &[0], &incoming, |f| Some(format!("lib{f}")));
        let said = |id: &str| (v[id].kind, v[id].library.as_str(), v[id].library_id.as_deref(), v[id].main_folder);
        assert_eq!(said("i1"), ("duplicate", "Arp - Zebra", Some("lib2"), None), "the other Incoming library's, not Replace's");
        assert_eq!(said("i2"), ("upgrade", "Arp/Zebra", Some("lib1"), None));
        assert!(!v.contains_key("i3"), "one whose folder Cratarr cannot see is held against the main libraries alone (it could be either copy)");
        // In Music too: Music's copy comes first, and Replace can take it.
        let libraries = [(0, LibraryIndex::new(vec![owned("Arp/Zebra (1999)", Quality::Lossy { kbps: 128 })])), (1, LibraryIndex::new(vec![owned("Arp/Zebra", Quality::Lossy { kbps: 320 })])), (2, LibraryIndex::new(vec![owned("Arp - Zebra", Quality::Lossless)]))];
        let v = verdicts(&albums, &libraries, &[0], &incoming, |f| Some(format!("lib{f}")));
        let said = |id: &str| (v[id].kind, v[id].library.as_str(), v[id].library_id.as_deref(), v[id].main_folder);
        assert_eq!(said("i1"), ("upgrade", "Arp/Zebra (1999)", Some("lib0"), Some(0)));
        assert_eq!(said("i2"), ("upgrade", "Arp/Zebra (1999)", Some("lib0"), Some(0)));
        assert_eq!(said("i3"), ("upgrade", "Arp/Zebra (1999)", Some("lib0"), Some(0)));
        assert_eq!(v["i1"].replaceable_into, ["lib0"]);
    }

    /// 0101 point 2: main libraries are equal. An album in the second main library is Replace's there, and one whose
    /// folder Cratarr cannot see is held against every main library, whatever their folders' ids.
    #[test]
    fn an_album_in_any_main_library_can_be_replaced_there() {
        // Two main libraries (folders 0 and 1), the album in the second; Incoming at folder 2.
        let incoming = [dir(2, "Arp/Zebra")];
        let albums = [album("i1", "/data/lidarr/Arp/Zebra", "FLAC"), album("i3", "/elsewhere/Zebra", "FLAC")];
        let libraries = [(0, LibraryIndex::new(Vec::new())), (1, LibraryIndex::new(vec![owned("Arp/Zebra (1999)", Quality::Lossy { kbps: 128 })])), (2, LibraryIndex::new(vec![owned("Arp/Zebra", Quality::Lossless)]))];
        let v = verdicts(&albums, &libraries, &[0, 1], &incoming, |f| Some(format!("lib{f}")));
        assert_eq!((v["i1"].kind, v["i1"].main_folder), ("upgrade", Some(1)));
        assert_eq!(v["i1"].replaceable_into, ["lib1"]);
        assert_eq!(v["i3"].main_folder, Some(1), "held against the second main library too");
        // With one main library, that folder is another library: told, not Replace's.
        let v = verdicts(&albums, &libraries, &[0], &incoming, |f| Some(format!("lib{f}")));
        assert_eq!((v["i1"].main_folder, v["i1"].replaceable_into.is_empty()), (None, true));
        assert!(!v.contains_key("i3"));
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
}

/// Where a record's copy in catalogue folder `folder` lies. Cratarr's own alone: another server's id is no record.
pub(super) async fn copy_dir_in(album: &str, folder: i64) -> Option<String> {
    let (served, id) = (crate::serve::instance()?, album.to_string());
    let copies = tokio::task::spawn_blocking(move || served.library.copies_of(&id)).await.ok()?.ok()?;
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
