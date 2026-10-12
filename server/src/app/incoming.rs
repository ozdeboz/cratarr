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
use crate::journal::{Entry, Event, EventId, Journal};
use crate::ports::{Acquisition, Files};

/// The handler name in the journal for telling Lidarr (decision 0011 §6). Its result is what it stopped fetching.
const ACQUISITION: &str = "acquisition";
/// What the acquisition handler is about to stop fetching, recorded before it acts (plan, then act).
const ACQUISITION_PLAN: &str = "acquisition-plan";
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
    /// What happened, for the handlers and for Undo (survives restarts).
    journal: Arc<Journal>,
    /// Whether Cratarr could write in a folder, and when that was tested (epoch seconds).
    writable: Mutex<std::collections::HashMap<PathBuf, (i64, bool)>>,
    /// Saved with the sources.
    trash_days: std::sync::atomic::AtomicU32,
}

impl IncomingService {
    pub(super) fn new(
        lib: Arc<Library>,
        files: Arc<dyn Files>,
        journal: Arc<Journal>,
        lidarr: Option<settings::LidarrConfig>,
        trash_days: u32,
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
        // Write-ahead: the journal knows what is about to happen before it does (the token is known up front), so a
        // crash in between is finished or forgotten by `reconcile`, never lost. Lidarr hears of it once applied.
        let now = epoch_now();
        let token = crate::trash::token(now, &dir.dir);
        let event = Event::AlbumRejected {
            source: source_id.to_string(),
            album_id: album.id.clone(),
            title: album.title.clone(),
            folder: dir.folder,
            dir: dir.dir.clone(),
            token: token.clone(),
            release_group: dir.release_group.clone(),
        };
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
                // Keep and Replace reconcile once they exist (decision 0013); until then nothing records them.
                _ => false,
            };
            let (title, _) = entry.event.album();
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

    /// React to a journal event (decision 0011 §6). Safe to call twice: what is done is recorded per event.
    pub(super) async fn handle(&self, entry: &Entry) {
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
        match self.journal.pending(ACQUISITION, since) {
            Ok(entries) => {
                for e in &entries {
                    self.handle(e).await;
                }
            }
            Err(e) => tracing::warn!("journal: {e:#}"),
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
