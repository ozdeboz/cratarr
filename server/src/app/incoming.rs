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

/// One user's data on one moved file's old server item, to give to its new item.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PlayData {
    source: String,
    track: i64,
    old_item: String,
    user: String,
    data: serde_json::Value,
}

/// What Keep would do with an album, for the page to show before asking.
#[derive(Debug, Serialize)]
pub struct KeepPreview {
    pub title: String,
    /// The album's new folder in Music, when it can be named.
    pub folder: Option<String>,
    /// How many files move (music and others).
    pub files: usize,
    /// Why Keep cannot go ahead as it is (`needs_year`: the page asks for the year and tries again).
    pub refused: Option<KeepRefused>,
    /// Hi-res files converted to CD format first (Keep then waits in the queue).
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
    /// In Music: its id now.
    Moved(String),
    /// In Music, but the server has not added it yet (its title).
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

/// The files of an album (`rels`, inside `dir`) that are lossless beyond CD format: converted on the way in (decision
/// 0013). Reads only their headers. Blocking.
fn to_convert(root: &std::path::Path, dir: &str, rels: &[String]) -> Vec<String> {
    rels.iter().filter(|rel| crate::convert::needs_conversion(&root.join(dir).join(rel)).ok().flatten().is_some()).cloned().collect()
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
    /// Per album id, when the album is already in the library: an upgrade of it or a duplicate (new albums are absent).
    pub verdicts: BTreeMap<String, VerdictView>,
    /// The albums Cratarr can delete: it sees their folder and can write there.
    pub deletable: Vec<String>,
    /// Per album id, what it lacks of what the library requires (the owner, 2026-10-24: marked before Keep).
    pub lacks: BTreeMap<String, Vec<&'static str>>,
    /// The libraries it was read from (the filter's library switches).
    pub libraries: Vec<String>,
}

/// How an incoming album relates to the library.
#[derive(Debug, Clone, Serialize)]
pub struct VerdictView {
    /// "upgrade" or "duplicate".
    pub kind: &'static str,
    /// The library album it matches (its folder).
    pub library: String,
    pub why: String,
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

/// Each album already in the library, by album id: an upgrade of it or a duplicate. The album's release group comes
/// from the catalogue, matched by the end of its folder path (the server's paths and Cratarr's differ in front).
fn verdicts(
    albums: &[source::Album],
    library: &crate::incoming::LibraryIndex,
    incoming: &[IncomingDir],
) -> BTreeMap<String, VerdictView> {
    use crate::incoming::{AlbumFacts, Verdict, quality_of};
    let mut out = BTreeMap::new();
    for a in albums {
        let release_group = a.path.as_deref().and_then(|p| incoming_dir(incoming, p)).and_then(|d| d.release_group.clone());
        let facts = AlbumFacts {
            release_group,
            artist: a.artist.clone().unwrap_or_default(),
            title: a.title.clone(),
            tracks: a.tracks,
            quality: quality_of(a.format.as_deref(), a.bitrate_kbps),
        };
        let (kind, i, why) = match library.classify(&facts) {
            Verdict::New => continue,
            Verdict::Upgrade { library, why } => ("upgrade", library, why),
            Verdict::Duplicate { library, why } => ("duplicate", library, why),
        };
        out.insert(a.id.clone(), VerdictView { kind, library: library.albums[i].0.clone(), why });
    }
    out
}

/// The catalogue's album directory behind a server path (the server's paths and Cratarr's differ in front).
fn incoming_dir<'a>(incoming: &'a [IncomingDir], path: &str) -> Option<&'a IncomingDir> {
    incoming.iter().filter(|d| !d.dir.is_empty() && path.ends_with(&format!("/{}", d.dir))).max_by_key(|d| d.dir.len())
}

/// How long a folder's write test is trusted (a remount needs a restart anyway, but ownership can be fixed live).
const WRITE_TEST_SECS: i64 = 600;

/// How long the library index (for Incoming's verdicts) is kept before it is read from the catalogue again.
const INDEX_SECS: i64 = 300;

/// More than any incoming queue should hold; the page shows them all.
const INCOMING_LIMIT: usize = 1000;

/// The library index plus the incoming albums' release groups by folder, read together from the catalogue.
struct Owned {
    built: i64,
    library: Arc<crate::incoming::LibraryIndex>,
    incoming: Arc<Vec<IncomingDir>>,
}

/// An album directory under an Incoming folder, as the catalogue knows it.
#[derive(Debug, Clone)]
struct IncomingDir {
    /// Catalogue folder id (1 and up are Incoming).
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

pub struct IncomingService {
    lib: Arc<Library>,
    files: Arc<dyn Files>,
    /// Incoming lists by source and library (see `refresh`).
    cache: Mutex<std::collections::HashMap<String, IncomingSourceView>>,
    /// See `Owned`; `None` until first needed, and after settings change.
    owned: Mutex<Option<Owned>>,
    /// Optional, from Settings: asked to stop fetching albums deleted from Incoming. Swapped in whole on save.
    lidarr: std::sync::RwLock<LidarrPair>,
    /// Incoming albums deleted here, hidden until the server's list no longer has them.
    hidden: Mutex<std::collections::HashSet<String>>,
    /// What happened, for the handlers and for Undo (survives restarts).
    journal: Arc<Journal>,
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
            view.albums.retain(|a| !hidden.contains(&a.id));
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
        if let Some((library, incoming)) = self.owned_index().await {
            let folders = self.lib.catalogue().folders();
            for view in &mut sources {
                view.verdicts = verdicts(&view.albums, &library, &incoming);
                let mut deletable = Vec::new();
                for a in &view.albums {
                    let folder = a.path.as_deref().and_then(|p| incoming_dir(&incoming, p)).and_then(|d| folders.get(d.folder as usize));
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

    /// The library index and the incoming release groups, read from the catalogue at most every `INDEX_SECS`.
    async fn owned_index(&self) -> Option<(Arc<crate::incoming::LibraryIndex>, Arc<Vec<IncomingDir>>)> {
        let mut owned = self.owned.lock().await;
        if let Some(o) = owned.as_ref().filter(|o| epoch_now() - o.built < INDEX_SECS) {
            return Some((o.library.clone(), o.incoming.clone()));
        }
        let c = self.lib.catalogue();
        let read = tokio::task::spawn_blocking(move || -> Result<(crate::incoming::LibraryIndex, Vec<IncomingDir>)> {
            use crate::incoming::{AlbumFacts, LibraryIndex, quality_of};
            let music = c.album_facts(0)?;
            let library = music
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
            let mut incoming = Vec::new();
            for id in 1..c.folders().len() as i64 {
                incoming.extend(c.album_facts(id)?.into_iter().map(|f| IncomingDir { folder: id, dir: f.dir, release_group: f.release_group }));
            }
            Ok((LibraryIndex::new(library), incoming))
        })
        .await;
        match read {
            Ok(Ok((library, incoming))) => {
                let o = Owned { built: epoch_now(), library: Arc::new(library), incoming: Arc::new(incoming) };
                let out = (o.library.clone(), o.incoming.clone());
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
        let ids = |v: &IncomingSourceView| v.albums.iter().map(|a| a.id.clone()).collect::<std::collections::BTreeSet<_>>();
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
        let listed: std::collections::HashSet<String> =
            self.cache.lock().await.values().flat_map(|v| v.albums.iter().map(|a| a.id.clone())).collect();
        self.hidden.lock().await.retain(|id| listed.contains(id));
    }

    /// Delete an Incoming album: into the trash inside its folder (Undo brings it back; emptied after
    /// the trash days set in Settings), and Lidarr stops fetching it.
    pub async fn delete(&self, source_id: &str, album_id: &str) -> Result<DeleteResult, ToggleError> {
        self.refuse_if_queued(album_id)?;
        let album = self
            .cache
            .lock()
            .await
            .values()
            .filter(|v| v.id == source_id)
            .flat_map(|v| v.albums.iter())
            .find(|a| a.id == album_id)
            .cloned()
            .ok_or_else(|| ToggleError::Refused("that album is no longer in Incoming".into()))?;
        self.delete_listed(source_id, album).await
    }

    /// Whether Music's albums can be deleted: Cratarr writes in Music (its trash is its own).
    pub(super) async fn music_deletable(&self) -> bool {
        let Some(music) = self.lib.catalogue().folders().first().cloned() else { return false };
        self.can_write(&music).await
    }

    /// Delete a listed album of any used library (decision 0034): into that library's own trash (each library keeps
    /// its own, the owner, 2026-10-16), Undo for the trash days.
    pub async fn delete_listed(&self, source_id: &str, album: source::Album) -> Result<DeleteResult, ToggleError> {
        self.refuse_if_queued(&album.id)?;
        let at = self.locate_listed(source_id, album).await?;
        let (album, dir, folder) = (at.album, at.dir, at.root);
        let catalogue = self.lib.catalogue();
        if !self.can_write(&folder).await {
            return Err(ToggleError::Refused("Cratarr cannot write in that library".into()));
        }
        // Write-ahead: the journal knows what is about to happen before it does (the token is known up front), so a
        // crash in between is finished or forgotten by `reconcile`, never lost. Lidarr hears of it once applied, and only
        // for an album that leaves an Incoming folder (Music stays out of it).
        let now = epoch_now();
        let token = crate::trash::token(now, &dir.dir);
        let event = Event::AlbumRejected {
            source: source_id.to_string(),
            album_id: album.id.clone(),
            title: album.title.clone(),
            folder: dir.folder,
            dir: dir.dir.clone(),
            token: token.clone(),
            release_group: if dir.folder != 0 { dir.release_group.clone() } else { None },
        };
        // Deleting the album that plays: the player stops first (a player going on with an item the server then drops
        // reports on it, which Jellyfin refuses; decision 0013). Nothing is played after: it is gone.
        let (c, f, d) = (catalogue.clone(), dir.folder, dir.dir.clone());
        let tracks: Vec<i64> = tokio::task::spawn_blocking(move || {
            c.album_tags(f, &d).unwrap_or_default().into_iter().filter_map(|(rel, _)| c.track_of(f, &rel)).collect()
        })
        .await
        .unwrap_or_default();
        if let Some(mut p) = self.playing_among(&tracks, &tracks.iter().map(|t| (*t, *t)).collect::<Vec<_>>()).await {
            self.hold(&mut p).await;
        }
        let id = self.journal.intend(now, &event)?;
        let (f, rel, files) = (folder.clone(), dir.dir.clone(), self.files.clone());
        let trashed = tokio::task::spawn_blocking(move || files.trash(&f, &rel, now)).await.map_err(|e| anyhow::anyhow!("{e}"));
        if let Err(e) = trashed.and_then(|r| r) {
            self.journal.forget(id)?;
            return Err(ToggleError::Refused(format!("{e:#}")));
        }
        self.finish_reject(&event).await;
        self.journal.applied(id)?;
        self.hidden.lock().await.insert(album.id.clone());
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

    /// Actions the journal intended but does not know to have happened (a crash in between): what the disk shows
    /// decides. Done → finish the bookkeeping and mark it applied (the handlers then run); not done → forget it.
    pub(super) async fn reconcile(&self) {
        let intents = match self.journal.intents() {
            Ok(i) => i,
            Err(e) => {
                tracing::warn!("journal: {e:#}");
                return;
            }
        };
        for entry in intents {
            let happened = match &entry.event {
                Event::AlbumRejected { folder, token, .. } => {
                    let root = self.lib.catalogue().folders().get(*folder as usize).cloned();
                    root.is_some_and(|r| crate::trash::holds(&r, token))
                }
                // Keep: it happened when any file reached Music; then it is finished forward (the moves repeat safely).
                Event::AlbumAdopted { moves, .. } => {
                    let folders = self.lib.catalogue().folders();
                    moves.iter().any(|m| folders.get(m.to.0 as usize).is_some_and(|f| f.join(&m.to.1).is_file()))
                }
                // Replace: it happened when the old files' trash entry exists (they move first).
                Event::AlbumReplaced { from_dir, replaced, token, .. } => {
                    let trash = replaced.first().map_or(from_dir.0, |r| r.to.0);
                    let root = self.lib.catalogue().folders().get(trash as usize).cloned();
                    root.is_some_and(|r| crate::trash::holds(&r, token))
                }
            };
            let (title, _) = entry.event.album();
            if happened && matches!(entry.event, Event::AlbumAdopted { .. } | Event::AlbumReplaced { .. }) {
                if let Some(steps) = self.steps(&entry.event) {
                    let files = self.files.clone();
                    let done = tokio::task::spawn_blocking(move || files.move_all(&steps)).await.map_err(|e| anyhow::anyhow!("{e}"));
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
        let Event::AlbumRejected { album_id, title, folder, dir, token, .. } = &entry.event else { return Err(gone()) };
        if entry.undone {
            return Err(ToggleError::Refused(format!("{title} is already back")));
        }
        let catalogue = self.lib.catalogue();
        let root = catalogue
            .folders()
            .get(*folder as usize)
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
        self.hidden.lock().await.remove(album_id);
        catalogue.poke();
        Ok(title.clone())
    }

    /// Find a listed album on disk, in any of the catalogue's folders (Music's too).
    async fn locate_listed(&self, source_id: &str, album: source::Album) -> Result<Located, ToggleError> {
        let path = album.path.clone().ok_or_else(|| ToggleError::Refused("the server gives no folder for that album".into()))?;
        let (library, incoming) = self.owned_index().await.ok_or_else(|| ToggleError::Refused("the catalogue is not ready yet".into()))?;
        let music: Vec<IncomingDir> =
            library.albums.iter().map(|(dir, f)| IncomingDir { folder: 0, dir: dir.clone(), release_group: f.release_group.clone() }).collect();
        let dir = incoming_dir(&incoming, &path)
            .or_else(|| incoming_dir(&music, &path))
            .cloned()
            .ok_or_else(|| ToggleError::Refused("Cratarr cannot see that album's folder".into()))?;
        let root = self.lib.catalogue().folders().get(dir.folder as usize).cloned().ok_or_else(|| ToggleError::Refused("that folder is not in use any more".into()))?;
        Ok(Located { source: source_id.to_string(), album, dir, root })
    }

    /// Find an Incoming album on disk.
    async fn locate(&self, source_id: &str, album_id: &str) -> Result<Located, ToggleError> {
        let album = self
            .cache
            .lock()
            .await
            .values()
            .filter(|v| v.id == source_id)
            .flat_map(|v| v.albums.iter())
            .find(|a| a.id == album_id)
            .cloned()
            .ok_or_else(|| ToggleError::Refused("that album is no longer in Incoming".into()))?;
        let path = album.path.clone().ok_or_else(|| ToggleError::Refused("the server gives no folder for that album".into()))?;
        let (_, incoming) = self.owned_index().await.ok_or_else(|| ToggleError::Refused("the catalogue is not ready yet".into()))?;
        let dir = incoming_dir(&incoming, &path)
            .cloned()
            .ok_or_else(|| ToggleError::Refused("Cratarr cannot see that album's folder".into()))?;
        let root = self
            .lib
            .catalogue()
            .folders()
            .get(dir.folder as usize)
            .cloned()
            .ok_or_else(|| ToggleError::Refused("that Incoming folder is not in use any more".into()))?;
        Ok(Located { source: source_id.to_string(), album, dir, root })
    }

    /// Work out Keep for an album: its new names, or why not. Reads the disk and the catalogue; changes nothing.
    async fn plan_keep(&self, at: &Located, year: Option<&str>) -> Result<(PathBuf, Result<crate::incoming::KeepPlan, KeepRefused>), ToggleError> {
        let music = PathBuf::from(self.lib.music().ok_or_else(|| ToggleError::Refused("no Music library is set".into()))?);
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
                Ok(plan) if music_.join(&plan.folder).exists() => {
                    Err(KeepRefused::Other(format!("Music already has a folder named {}", plan.folder)))
                }
                Ok(plan) => Ok(plan),
                Err(crate::incoming::KeepRefusal::NeedsYear) => Err(KeepRefused::NeedsYear),
                Err(e) => Err(KeepRefused::Other(e.to_string())),
            })
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        Ok((music, plan))
    }

    /// What Keep would do with an album (decision 0013), for the page to show before asking. Changes nothing.
    pub async fn keep_preview(&self, source_id: &str, album_id: &str, year: Option<&str>) -> Result<KeepPreview, ToggleError> {
        let at = self.locate(source_id, album_id).await?;
        let (_, plan) = self.plan_keep(&at, year).await?;
        let convert = self.to_convert(&at).await.len();
        let acquisition = self.acquisition().is_some();
        Ok(match plan {
            Ok(p) => KeepPreview { title: at.album.title, folder: Some(p.folder), files: p.moves.len(), refused: None, convert, acquisition },
            Err(r) => KeepPreview { title: at.album.title, folder: None, files: 0, refused: Some(r), convert, acquisition },
        })
    }

    /// The album's music files that need converting first.
    async fn to_convert(&self, at: &Located) -> Vec<String> {
        let (c, folder, dir, root) = (self.lib.catalogue(), at.dir.folder, at.dir.dir.clone(), at.root.clone());
        tokio::task::spawn_blocking(move || {
            let prefix = format!("{dir}/");
            let rels: Vec<String> = c
                .album_tags(folder, &dir)
                .unwrap_or_default()
                .into_iter()
                .map(|(rel, _)| rel.strip_prefix(&prefix).unwrap_or(&rel).to_string())
                .collect();
            to_convert(&root, &dir, &rels)
        })
        .await
        .unwrap_or_default()
    }

    /// Keep (decision 0013): move a new album from Incoming into Music, named per the library's template. Write-ahead
    /// like Delete; the files and catalogue rows move (track ids kept); Lidarr stops fetching it, and the moved
    /// files' new server items get the old ones' play data (handlers). Undo puts it all back.
    /// Hi-res files are converted first, in the queue: the plan is checked now, so a refusal shows at once.
    pub async fn keep(&self, source_id: &str, album_id: &str, year: Option<&str>) -> Result<KeepResult, ToggleError> {
        self.refuse_if_queued(album_id)?;
        let at = self.locate(source_id, album_id).await?;
        self.move_located(source_id, at, 0, settings::Format::Cd, year).await
    }

    /// Move (decision 0034): an album of any used library into another of its server; Keep is the move into Music.
    /// The target's policies apply on the way in: its naming (Music: the template; another library: the album's
    /// folder as it is) and its format (CD format: hi-res converted first, in the queue). `album` is the listed album
    /// (Music's or another library's).
    pub async fn move_album(&self, source_id: &str, album: source::Album, to: &str, year: Option<&str>) -> Result<KeepResult, ToggleError> {
        self.refuse_if_queued(&album.id)?;
        let at = self.locate_listed(source_id, album).await?;
        let (target, format) = self.target_of(source_id, to)?;
        if target == at.dir.folder {
            return Err(ToggleError::Refused("the album is in that library already".into()));
        }
        self.move_located(source_id, at, target, format, year).await
    }

    /// What a move would do (decision 0034), for the page to show before asking. Changes nothing.
    pub async fn move_preview(&self, source_id: &str, album: source::Album, to: &str, year: Option<&str>) -> Result<KeepPreview, ToggleError> {
        let at = self.locate_listed(source_id, album).await?;
        let (target, format) = self.target_of(source_id, to)?;
        let (_, plan) = self.plan_move(&at, target, year).await?;
        let convert = if format == settings::Format::Cd { self.to_convert(&at).await.len() } else { 0 };
        let acquisition = self.acquisition().is_some() && at.dir.folder != 0;
        Ok(match plan {
            Ok(p) => KeepPreview { title: at.album.title, folder: Some(p.folder), files: p.moves.len(), refused: None, convert, acquisition },
            Err(r) => KeepPreview { title: at.album.title, folder: None, files: 0, refused: Some(r), convert, acquisition },
        })
    }

    async fn move_located(&self, source_id: &str, at: Located, target: i64, format: settings::Format, year: Option<&str>) -> Result<KeepResult, ToggleError> {
        let (root, plan) = self.plan_move(&at, target, year).await?;
        let plan = plan.map_err(|r| match r {
            KeepRefused::NeedsYear => ToggleError::Refused("the album's year is not known".into()),
            KeepRefused::Other(why) => ToggleError::Refused(why),
        })?;
        if !self.can_write(&at.root).await || !self.can_write(&root).await {
            return Err(ToggleError::Refused("Cratarr cannot write in both libraries".into()));
        }
        let convert = if format == settings::Format::Cd { self.to_convert(&at).await } else { Vec::new() };
        if !convert.is_empty() {
            return Ok(self.enqueue(source_id, at, queue::Action::Keep, year, convert, plan.folder, target));
        }
        self.keep_now(&at, target, year, Vec::new(), None).await
    }

    /// A library of a server as Cratarr's catalogue folder (0: the Music folder), and its format policy.
    fn target_of(&self, source_id: &str, library: &str) -> Result<(i64, settings::Format), ToggleError> {
        let live = self.lib.live();
        let a = live.iter().find(|a| a.cfg.id == source_id).ok_or_else(|| ToggleError::Refused("that server is not connected".into()))?;
        let roles = &a.cfg.libraries;
        if !roles.music.iter().chain(&roles.incoming).any(|l| l == library) {
            return Err(ToggleError::Refused("that library is not used".into()));
        }
        let format = roles.format_of(library);
        if roles.music.iter().any(|l| l == library) {
            return Ok((0, format));
        }
        let placed = a.placement.libraries.iter().find(|l| l.id == library).and_then(|l| l.folders.first()).ok_or_else(|| ToggleError::Refused("Cratarr cannot see that library's files".into()))?;
        let folders = self.lib.catalogue().folders();
        let i = folders.iter().position(|f| f == &PathBuf::from(placed)).ok_or_else(|| ToggleError::Refused("that library's folder is not in use yet".into()))?;
        Ok((i as i64, format))
    }

    /// Work out a move's names, or why not: into Music by the template (Keep's plan), into another library as the album's
    /// folder is. Reads the disk and the catalogue; changes nothing.
    async fn plan_move(&self, at: &Located, target: i64, year: Option<&str>) -> Result<(PathBuf, Result<crate::incoming::KeepPlan, KeepRefused>), ToggleError> {
        if target == 0 {
            return self.plan_keep(at, year).await;
        }
        let root = self.lib.catalogue().folders().get(target as usize).cloned().ok_or_else(|| ToggleError::Refused("that library's folder is not in use".into()))?;
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

    /// Keep, its files ready (`converted`: what conversion did just before). `job`: the queued job doing it, told
    /// the event's id as soon as it is intended.
    async fn keep_now(
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
        // Before anything moves: what users did with the old server items (the server forgets them after).
        let mut play: Vec<PlayData> = Vec::new();
        for (source, s) in self.lib.connected_sources() {
            let rels: Vec<String> = moves.iter().map(|m| m.from.1.clone()).collect();
            let items = c.track_ids(&source, at.dir.folder, &rels).unwrap_or_default();
            for (m, item) in moves.iter().zip(items) {
                let (Some(track), Some(item)) = (m.track, item) else { continue };
                match s.user_data(&item).await {
                    Ok(users) => play.extend(users.into_iter().map(|(user, data)| PlayData {
                        source: source.clone(),
                        track,
                        old_item: item.clone(),
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
            // Lidarr stops fetching an album that leaves a folder it fetches into: an Incoming one (Music stays out of
            // it); with no release group it is not asked.
            release_group: if at.dir.folder != 0 { at.dir.release_group.clone() } else { None },
            converted,
        };
        let now = epoch_now();
        let id = self.journal.intend(now, &event)?;
        if let Some(job) = job {
            self.queue.intended(job, id);
        }
        self.journal.done(id, PLAYDATA_PLAN, now, &serde_json::to_value(&play).map_err(anyhow::Error::from)?)?;
        // Playing from this album: carried on with its new copy after the move (the old items go).
        let order: Vec<i64> = match &event {
            Event::AlbumAdopted { moves, .. } => moves.iter().filter_map(|m| m.track).collect(),
            _ => Vec::new(),
        };
        let mut playing = self.playing_among(&order, &order.iter().map(|t| (*t, *t)).collect::<Vec<_>>()).await;
        if let Some(p) = playing.as_mut() {
            self.hold(p).await;
        }
        let steps = self.steps(&event).ok_or_else(|| ToggleError::Refused("the folders are not in use any more".into()))?;
        let files = self.files.clone();
        let moved = tokio::task::spawn_blocking(move || files.move_all(&steps)).await.map_err(|e| anyhow::anyhow!("{e}"));
        if let Err(e) = moved.and_then(|r| r) {
            self.journal.forget(id)?;
            return Err(ToggleError::Refused(format!("{e:#}")));
        }
        self.finish_adopt(&event, false, playing.is_some()).await;
        self.journal.applied(id)?;
        self.hidden.lock().await.insert(at.album.id.clone());
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
        let event = self.journal.album_fate(source_id, album_id).ok()??;
        let (moves, title) = match &event {
            Event::AlbumAdopted { moves, title, .. } | Event::AlbumReplaced { moves, title, .. } => (moves, title),
            Event::AlbumRejected { title, .. } => return Some(AlbumFate::Deleted(title.clone())),
        };
        let s = self.lib.connected_sources().into_iter().find(|(id, _)| id == source_id).map(|(_, s)| s)?;
        // Its tracks' new items (once a sync pass linked them), and the album the server put them in.
        let Some(folder) = moves.first().map(|m| m.to.0) else { return Some(AlbumFate::NotYet(title.clone())) };
        let rels: Vec<String> = moves.iter().filter(|m| m.track.is_some()).map(|m| m.to.1.clone()).collect();
        let items = self.lib.catalogue().track_ids(source_id, folder, &rels).unwrap_or_default();
        for item in items.into_iter().flatten().take(3) {
            if let Ok(t) = s.track(&item).await
                && let Some(album) = t.album_id
            {
                return Some(AlbumFate::Moved(album));
            }
        }
        Some(AlbumFate::NotYet(title.clone()))
    }

    /// Refused while the album waits in the conversion queue (Cancel it first).
    fn refuse_if_queued(&self, album_id: &str) -> Result<(), ToggleError> {
        match self.queue.busy(album_id) {
            true => Err(ToggleError::Refused("that album is being converted".into())),
            false => Ok(()),
        }
    }

    /// The moves that undo an album action, in order: its own moves back, then any conversion undone (the converted
    /// files into the trash, the originals back from it). One plan, so all of it happens or none. `None` when a
    /// folder is no longer in use.
    fn undo_steps(&self, event: &Event) -> Option<Vec<crate::moves::Step>> {
        let mut back: Vec<crate::moves::Step> =
            self.steps(event)?.into_iter().rev().map(|s| crate::moves::Step { from: s.to, to: s.from }).collect();
        let (Event::AlbumAdopted { from_dir, converted, .. } | Event::AlbumReplaced { from_dir, converted, .. }) = event else {
            return Some(back);
        };
        let root = self.lib.catalogue().folders().get(from_dir.0 as usize)?.clone();
        let at = |rel: &str| (root.clone(), rel.to_string());
        for c in converted {
            back.push(crate::moves::Step { from: at(&c.converted), to: at(&queue::converted_trash(c)?) });
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
                    from: (folders.get(m.from.0 as usize)?.clone(), m.from.1.clone()),
                    to: (folders.get(m.to.0 as usize)?.clone(), m.to.1.clone()),
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
        let Event::AlbumAdopted { album_id, title, .. } = &entry.event else { unreachable!() };
        if entry.undone {
            return Err(ToggleError::Refused(format!("{title} is already back where it was")));
        }
        let steps = self.undo_steps(&entry.event).ok_or_else(|| ToggleError::Refused("the folders are not in use any more".into()))?;
        let files = self.files.clone();
        tokio::task::spawn_blocking(move || files.move_all(&steps))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
        self.finish_adopt(&entry.event, true, false).await;
        self.unconvert_rows(&entry.event).await;
        self.journal.mark_undone(entry.id, epoch_now())?;
        self.resume_fetching(entry.id, title).await;
        self.hidden.lock().await.remove(album_id);
        *self.owned.lock().await = None;
        self.lib.catalogue().poke();
        Ok(title.clone())
    }

    /// Give the moved files' new server items the play data their old items had (read before the move). Waits until
    /// the server has the new items and Cratarr has linked them (a scan, then a sync pass); safe to repeat.
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
            match c.source_id_of(&p.source, p.track) {
                Some(new) if new != p.old_item => ready.push((s.clone(), new, p)),
                _ => return, // not scanned or linked yet: the next pass tries again
            }
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
        let folders: Vec<std::path::PathBuf> = self.lib.catalogue().folders();
        let (now, keep, files) = (epoch_now(), self.trash_days(), self.files.clone());
        let gone = tokio::task::spawn_blocking(move || {
            folders.iter().map(|f| files.purge(f, now, keep).unwrap_or_else(|e| {
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
            .filter(|(a, _)| !a.cfg.libraries.incoming.is_empty())
            .map(|(a, s)| {
                let ids = a.cfg.libraries.incoming.clone();
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
            let f = folders.get(*folder as usize)?.display().to_string();
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
