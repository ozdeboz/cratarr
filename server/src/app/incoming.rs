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
use crate::ports::{Acquisition, Files};
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
}

/// Lidarr on the Settings page. The key is never sent.
#[derive(Debug, Serialize)]
pub struct LidarrView {
    pub url: String,
    pub key_set: bool,
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

/// What Delete did, so Undo can reverse it.
#[derive(Debug, Clone)]
struct Deleted {
    folder: i64,
    /// The album folder, relative to `folder`.
    dir: String,
    token: String,
    album_id: String,
    title: String,
    /// Lidarr albums that were monitored and were switched off.
    lidarr_ids: Vec<i64>,
}

/// A Lidarr connection and its client (`None` when the key cannot be used in a header).
pub(super) type LidarrPair = (Option<settings::LidarrConfig>, Option<crate::lidarr::Lidarr>);

pub(super) fn lidarr_pair(cfg: Option<settings::LidarrConfig>) -> LidarrPair {
    let client = cfg.as_ref().and_then(|c| match crate::lidarr::Lidarr::new(&c.url, &c.api_key) {
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
    /// Deletions that Undo can still reverse, by undo key.
    deleted: Mutex<std::collections::HashMap<String, Deleted>>,
    /// Whether Cratarr could write in a folder, and when that was tested (epoch seconds).
    writable: Mutex<std::collections::HashMap<PathBuf, (i64, bool)>>,
    /// Saved with the sources.
    trash_days: std::sync::atomic::AtomicU32,
}

impl IncomingService {
    pub(super) fn new(lib: Arc<Library>, files: Arc<dyn Files>, lidarr: Option<settings::LidarrConfig>, trash_days: u32) -> Self {
        Self {
            lib,
            files,
            cache: Mutex::new(std::collections::HashMap::new()),
            owned: Mutex::new(None),
            lidarr: std::sync::RwLock::new(lidarr_pair(lidarr)),
            hidden: Mutex::new(std::collections::HashSet::new()),
            deleted: Mutex::new(std::collections::HashMap::new()),
            writable: Mutex::new(std::collections::HashMap::new()),
            trash_days: std::sync::atomic::AtomicU32::new(trash_days),
        }
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
            Some(l) => match l.version().await {
                Ok(v) => (Some(v), None),
                Err(e) => (None, Some(format!("{e:#}"))),
            },
            None => (None, Some("the API key is not usable".into())),
        };
        Some(LidarrView { url: cfg.url.clone(), key_set: !cfg.api_key.is_empty(), version, error })
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
            let kept = self.cache.lock().await.get(&key).cloned();
            let view = match kept {
                Some(v) => v,
                None => {
                    let v = Self::read_incoming(&a, &*s, &libraries).await;
                    self.cache.lock().await.insert(key, v.clone());
                    v
                }
            };
            sources.push(view);
        }
        let hidden = self.hidden.lock().await.clone();
        for view in &mut sources {
            view.albums.retain(|a| !hidden.contains(&a.id));
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
        IncomingView { sources, trash_days: self.trash_days() }
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
        let mut keep = std::collections::HashSet::new();
        for (a, s, libraries) in self.incoming_sources() {
            let key = format!("{}\n{}", a.cfg.id, libraries.join("\n"));
            let fresh = Self::read_incoming(&a, &*s, &libraries).await;
            let mut cache = self.cache.lock().await;
            if fresh.error.is_none() || !cache.contains_key(&key) {
                cache.insert(key.clone(), fresh);
            }
            keep.insert(key);
        }
        self.cache.lock().await.retain(|k, _| keep.contains(k));
        // A deleted album stays hidden until the server's fresh list no longer has it (or Undo brings it back).
        let listed: std::collections::HashSet<String> =
            self.cache.lock().await.values().flat_map(|v| v.albums.iter().map(|a| a.id.clone())).collect();
        self.hidden.lock().await.retain(|id| listed.contains(id));
    }

    /// Delete an Incoming album: into the trash inside its folder (Undo brings it back; emptied after
    /// the trash days set in Settings), and Lidarr stops fetching it.
    pub async fn delete(&self, source_id: &str, album_id: &str) -> Result<DeleteResult, ToggleError> {
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
        let catalogue = self.lib.catalogue();
        let folder = catalogue
            .folders()
            .get(dir.folder as usize)
            .cloned()
            .ok_or_else(|| ToggleError::Refused("that Incoming folder is not in use any more".into()))?;
        let (f, rel, now, files) = (folder.clone(), dir.dir.clone(), epoch_now(), self.files.clone());
        let token = tokio::task::spawn_blocking(move || files.trash(&f, &rel, now))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
        // Its tracks keep their ids while in the trash (decision 0011): Undo brings them back unchanged.
        let (c, f, rel, tok) = (catalogue.clone(), dir.folder, dir.dir.clone(), token.clone());
        if let Err(e) = tokio::task::spawn_blocking(move || c.mark_trashed(f, &rel, &tok)).await.map_err(|e| anyhow::anyhow!("{e}")).and_then(|r| r) {
            tracing::warn!("catalogue: could not mark {} as trashed: {e:#}", album.title);
        }
        // Best effort: the album is in the trash either way; a Lidarr failure only means it may come back.
        let lidarr_ids = match (self.acquisition(), dir.release_group.as_deref()) {
            (Some(l), Some(rg)) => l.stop_fetching(rg).await.unwrap_or_else(|e| {
                tracing::warn!("Lidarr: could not stop fetching {}: {e:#}", album.title);
                Vec::new()
            }),
            _ => Vec::new(),
        };
        self.hidden.lock().await.insert(album.id.clone());
        catalogue.poke();
        let key = format!("{}:{token}", dir.folder);
        self.deleted.lock().await.insert(
            key.clone(),
            Deleted { folder: dir.folder, dir: dir.dir.clone(), token, album_id: album.id.clone(), title: album.title.clone(), lidarr_ids },
        );
        Ok(DeleteResult { undo: key, title: album.title })
    }

    /// Undo a Delete: back from the trash, and Lidarr monitors what it monitored before.
    pub async fn undo_delete(&self, key: &str) -> Result<String, ToggleError> {
        let d = self
            .deleted
            .lock()
            .await
            .get(key)
            .cloned()
            .ok_or_else(|| ToggleError::Refused("that deletion can no longer be undone here".into()))?;
        let catalogue = self.lib.catalogue();
        let folder = catalogue
            .folders()
            .get(d.folder as usize)
            .cloned()
            .ok_or_else(|| ToggleError::Refused("that Incoming folder is not in use any more".into()))?;
        let (token, files) = (d.token.clone(), self.files.clone());
        tokio::task::spawn_blocking(move || files.restore(&folder, &token))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .map_err(|e| ToggleError::Refused(format!("{e:#}")))?;
        let (c, f, rel, tok) = (catalogue.clone(), d.folder, d.dir.clone(), d.token.clone());
        if let Err(e) = tokio::task::spawn_blocking(move || c.unmark_trashed(f, &rel, &tok)).await.map_err(|e| anyhow::anyhow!("{e}")).and_then(|r| r) {
            tracing::warn!("catalogue: could not bring {} back from the trash: {e:#}", d.title);
        }
        if let Some(l) = self.acquisition() {
            if let Err(e) = l.resume(&d.lidarr_ids).await {
                tracing::warn!("Lidarr: could not restore monitoring of {}: {e:#}", d.title);
            }
        }
        self.deleted.lock().await.remove(key);
        self.hidden.lock().await.remove(&d.album_id);
        catalogue.poke();
        Ok(d.title)
    }

    /// Empty Incoming trash older than the trash days set in Settings.
    pub(super) async fn purge_trash(&self) {
        let folders: Vec<std::path::PathBuf> = self.lib.catalogue().folders().into_iter().skip(1).collect();
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
            tracing::info!("emptied {gone} old deletion(s) from the Incoming trash");
            self.deleted.lock().await.clear();
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

    async fn read_incoming(a: &Active, s: &dyn Source, libraries: &[String]) -> IncomingSourceView {
        let supported = s.lists_albums();
        let mut albums = Vec::new();
        let mut error = None;
        if supported {
            for library in libraries {
                match s.albums(library, INCOMING_LIMIT).await {
                    Ok(list) => albums.extend(list),
                    Err(e) => {
                        error.get_or_insert(format!("{e:#}"));
                    }
                }
            }
            albums.sort_by_key(|a: &source::Album| std::cmp::Reverse(a.added.unwrap_or(0)));
        }
        IncomingSourceView { id: a.cfg.id.clone(), label: label_of(&a.cfg), supported, albums, error, as_of: epoch_now(), verdicts: BTreeMap::new(), deletable: Vec::new() }
    }

    /// Play an album of a server's incoming music on that server's player. Returns the player's label.
    pub async fn play(&self, source_id: &str, album_id: &str) -> Result<String> {
        anyhow::ensure!(crate::covers::valid_item_id(album_id), "not an album id");
        let (_, s, _) = self
            .incoming_sources()
            .into_iter()
            .find(|(a, _, _)| a.cfg.id == source_id)
            .ok_or_else(|| anyhow::anyhow!("that server has no Incoming library"))?;
        s.play_album(album_id).await
    }

    /// An album's image from a connected server: the Incoming list, and the now-playing header for music outside the
    /// library. Any connected server, not only one with an incoming library: the header needs the cover of whatever
    /// plays. Only images come back, and the id is only ever passed on to that server.
    pub async fn cover(&self, source_id: &str, album_id: &str) -> Option<(String, Vec<u8>)> {
        if !crate::covers::valid_item_id(album_id) {
            return None;
        }
        let (_, s) = self.lib.connected().into_iter().find(|(a, _)| a.cfg.id == source_id)?;
        match s.album_cover(album_id).await {
            Ok(found) => found,
            Err(e) => {
                tracing::warn!("cover of {album_id} on {source_id}: {e:#}");
                None
            }
        }
    }
}
