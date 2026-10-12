//! Application logic: follows what the sources play, works out which playlist files contain it, decides what a tap
//! does, and holds the live sources (swapped in whole by the Settings page).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;
use tokio::sync::Mutex;

use crate::config::Config;
use crate::nowplaying::Progress;
use crate::placement::{self, Placement};
use crate::playlists::{DupGroup, PlaylistView, Repair, Repairs, Store, StoreError};
use crate::settings::{self, ChangeError, Origin, SourceConfig};
use crate::source::{self, Choice, Field, Playing, PlaylistChange, PlaylistEvent, Source, Status};

#[derive(Debug, Serialize, PartialEq)]
pub struct NowPlayingView {
    /// Which source this came from; covers are looked up there.
    pub source: String,
    pub id: String,
    pub name: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// The source's album id: the Incoming page uses it to mark the album that is playing.
    pub album_id: Option<String>,
    /// False when the track's library is not in use (an incoming folder, say): it cannot be added.
    pub addable: bool,
    /// `None` when nothing reports a position.
    pub progress: Option<Progress>,
}

/// One enabled source, for the page's source picker.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SourceTab {
    pub id: String,
    pub label: String,
    pub playing: bool,
}

#[derive(Debug, Serialize)]
pub struct StateView {
    pub now_playing: Option<NowPlayingView>,
    pub playlists: Vec<PlaylistView>,
    pub sources: Vec<SourceTab>,
}

#[derive(Debug, thiserror::Error)]
pub enum ToggleError {
    #[error("nothing is playing")]
    NothingPlaying,
    #[error("the playing track is not in a library in use, so it cannot be added to a playlist")]
    NotInLibrary,
    #[error("unknown playlist")]
    UnknownPlaylist,
    #[error("that repair is no longer offered")]
    NotOffered,
    #[error("there is no earlier version to go back to")]
    NothingToUndo,
    #[error("a playlist with that name already exists")]
    Exists,
    #[error("that is not a usable playlist name")]
    BadName,
    #[error("settings cannot be saved: CRATARR_DATA_DIR is not set")]
    SettingsReadOnly,
    #[error("{0}")]
    BadSettings(String),
    #[error("{0:#}")]
    Upstream(#[from] anyhow::Error),
}

impl From<ChangeError> for ToggleError {
    fn from(e: ChangeError) -> Self {
        ToggleError::BadSettings(e.to_string())
    }
}

impl From<StoreError> for ToggleError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::Unknown => ToggleError::UnknownPlaylist,
            StoreError::NotOffered => ToggleError::NotOffered,
            StoreError::NothingToUndo => ToggleError::NothingToUndo,
            StoreError::Exists => ToggleError::Exists,
            StoreError::BadName => ToggleError::BadName,
            StoreError::Io(e) => ToggleError::Upstream(e),
        }
    }
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// One source's incoming library, newest first. `supported` is false when the source cannot list albums at all.
#[derive(Debug, Serialize)]
pub struct IncomingSourceView {
    pub id: String,
    pub label: String,
    pub supported: bool,
    pub albums: Vec<source::Album>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct IncomingView {
    pub sources: Vec<IncomingSourceView>,
}

/// More than any incoming queue should hold; the page shows them all.
const INCOMING_LIMIT: usize = 1000;

#[derive(Debug, Serialize)]
pub struct StatusView {
    pub connected: bool,
    /// Some enabled source has an incoming library: the page shows incoming-music features.
    pub incoming: bool,
    /// Connected sources, for the source picker (`playing` is not checked here and always false).
    pub sources: Vec<SourceTab>,
    /// The server's name with one source; "n of m sources" with several.
    pub server_name: Option<String>,
    pub version: &'static str,
}

/// Set by the container (port, mount), so shown but not editable.
#[derive(Debug, Serialize)]
pub struct FixedSettings {
    pub bind: String,
    pub playlist_dir: String,
}

#[derive(Debug, Serialize)]
pub struct KindView {
    pub id: &'static str,
    pub label: &'static str,
    pub fields: Vec<Field>,
}

#[derive(Debug, Serialize)]
pub struct SourceView {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub enabled: bool,
    /// What its libraries are used for.
    pub libraries: crate::placement::Roles,
    /// Secrets left out; see `secrets_set`.
    pub values: source::Values,
    pub secrets_set: Vec<String>,
    pub status: Status,
    pub choices: BTreeMap<String, Vec<Choice>>,
    pub placement: Placement,
    /// Why the source could not be connected or its choices not filled, if so.
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SettingsView {
    pub sources: Vec<SourceView>,
    pub kinds: Vec<KindView>,
    /// Whether the sources come from the settings file or (first run) the environment.
    pub origin: Origin,
    pub fixed: FixedSettings,
    /// `None` when CRATARR_DATA_DIR is unset: nothing can be saved.
    pub settings_file: Option<String>,
    pub playlists: usize,
    pub version: &'static str,
    /// Epoch seconds.
    pub started: i64,
}

fn epoch_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn label_of(cfg: &SourceConfig) -> String {
    source::kind(&cfg.kind).map_or_else(|| cfg.kind.clone(), |k| k.label.to_string())
}

/// A configured source and, when it is enabled and its settings work, the connection and where its music lies.
struct Active {
    cfg: SourceConfig,
    source: Option<Arc<dyn Source>>,
    placement: Placement,
    error: Option<String>,
}

impl Active {
    async fn start(cfg: SourceConfig, playlist_dir: &std::path::Path) -> Self {
        if !cfg.enabled {
            return Self { cfg, source: None, placement: Placement::default(), error: None };
        }
        let connected = source::kind(&cfg.kind)
            .ok_or_else(|| anyhow::anyhow!("unknown kind of source {:?}", cfg.kind))
            .and_then(|k| (k.connect)(&cfg.values, false));
        let source: Arc<dyn Source> = match connected {
            Ok(s) => Arc::from(s),
            Err(e) => {
                tracing::warn!("source {}: {e:#}", cfg.id);
                return Self { cfg, source: None, placement: Placement::default(), error: Some(format!("{e:#}")) };
            }
        };
        let placement = placement::place(&*source, playlist_dir).await.unwrap_or_else(|e| {
            tracing::warn!("source {}: cannot look at its libraries yet: {e:#}", cfg.id);
            Placement::default()
        });
        if placement.root.is_none() {
            tracing::warn!("source {}: no library found inside the playlist folder (yet)", cfg.id);
        }
        Self { cfg, source: Some(source), placement, error: None }
    }

    fn entry(&self, track: &source::Track) -> Option<String> {
        self.placement.entry(&self.cfg.libraries, track.library_id.as_deref(), track.path.as_deref())
    }
}

pub struct App {
    live: std::sync::RwLock<Arc<Vec<Arc<Active>>>>,
    origin: std::sync::RwLock<Origin>,
    playlist_dir: PathBuf,
    bind: String,
    settings_path: Option<PathBuf>,
    /// One settings change at a time, so two pages (or the background placement retry) cannot interleave.
    saving: Mutex<()>,
    started: i64,
    store: Store,
}

impl App {
    pub async fn new(cfg: Config, sources: Vec<SourceConfig>, origin: Origin) -> Arc<Self> {
        let playlist_dir = PathBuf::from(&cfg.playlist_dir);
        let mut active = Vec::new();
        for s in sources {
            active.push(Arc::new(Active::start(s, &playlist_dir).await));
        }
        Arc::new(Self {
            live: std::sync::RwLock::new(Arc::new(active)),
            origin: std::sync::RwLock::new(origin),
            settings_path: cfg.data_dir.as_ref().map(|d| PathBuf::from(d).join("settings.json")),
            store: Store::new(&cfg.playlist_dir),
            playlist_dir,
            bind: cfg.bind,
            saving: Mutex::new(()),
            started: epoch_now(),
        })
    }

    fn live(&self) -> Arc<Vec<Arc<Active>>> {
        self.live.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    fn connected(&self) -> Vec<(Arc<Active>, Arc<dyn Source>)> {
        self.live().iter().filter_map(|a| a.source.clone().map(|s| (a.clone(), s))).collect()
    }

    /// Sources that were down when they were set up get looked at again (called now and then).
    pub async fn retry_placement(&self) {
        let _one_at_a_time = self.saving.lock().await;
        let live = self.live();
        if !live.iter().any(|a| a.source.is_some() && a.placement.root.is_none()) {
            return;
        }
        let mut next = Vec::with_capacity(live.len());
        for a in live.iter() {
            match &a.source {
                Some(s) if a.placement.root.is_none() => {
                    let placement = placement::place(&**s, &self.playlist_dir).await.unwrap_or_default();
                    if placement.root.is_some() {
                        tracing::info!("source {}: found its libraries in the playlist folder", a.cfg.id);
                    }
                    next.push(Arc::new(Active { cfg: a.cfg.clone(), source: Some(s.clone()), placement, error: a.error.clone() }));
                }
                _ => next.push(a.clone()),
            }
        }
        *self.live.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(next);
    }

    /// For the navigation header.
    pub async fn status(&self) -> StatusView {
        let sources = self.connected();
        let mut statuses = Vec::new();
        for (_, s) in &sources {
            statuses.push(s.status().await.unwrap_or_default());
        }
        let up = statuses.iter().filter(|s| s.connected).count();
        let server_name = match statuses.as_slice() {
            [] => None,
            [one] => one.server_name.clone(),
            many => Some(format!("{up} of {} sources", many.len())),
        };
        let incoming = self.live().iter().any(|a| a.cfg.enabled && a.cfg.libraries.incoming.is_some());
        let sources = sources.iter().map(|(a, _)| SourceTab { id: a.cfg.id.clone(), label: label_of(&a.cfg), playing: false }).collect();
        StatusView { connected: up > 0, incoming, sources, server_name, version: VERSION }
    }

    /// Ask every connected source what it plays, all at once, and pick: the requested source, or (`None` / unknown)
    /// the one whose track started most recently.
    async fn playing(&self, want: Option<&str>) -> Result<(Option<(Arc<Active>, Playing)>, Vec<SourceTab>)> {
        let sources = self.connected();
        let mut set = tokio::task::JoinSet::new();
        for (i, (_, s)) in sources.iter().enumerate() {
            let s = s.clone();
            set.spawn(async move { (i, s.now_playing().await) });
        }
        let mut results: Vec<Option<Result<Option<Playing>>>> = (0..sources.len()).map(|_| None).collect();
        while let Some(done) = set.join_next().await {
            let (i, r) = done.map_err(|e| anyhow::anyhow!("asking a source what plays: {e}"))?;
            results[i] = Some(r);
        }
        let results: Vec<Result<Option<Playing>>> = results.into_iter().map(|r| r.unwrap_or(Ok(None))).collect();
        let tabs = sources
            .iter()
            .zip(&results)
            .map(|((a, _), r)| SourceTab { id: a.cfg.id.clone(), label: label_of(&a.cfg), playing: matches!(r, Ok(Some(_))) })
            .collect();
        let wanted = want.and_then(|w| sources.iter().position(|(a, _)| a.cfg.id == w));
        let mut first_error = None;
        let mut errors = 0;
        let mut best: Option<(usize, Playing)> = None;
        for (i, r) in results.into_iter().enumerate() {
            match r {
                Err(e) => {
                    tracing::warn!("source {}: {e:#}", sources[i].0.cfg.id);
                    if wanted == Some(i) {
                        return Err(e);
                    }
                    errors += 1;
                    first_error.get_or_insert(e);
                }
                Ok(Some(p)) if wanted.is_none_or(|w| w == i) => {
                    let newer = best.as_ref().is_none_or(|(_, b)| p.started_at.unwrap_or(0) > b.started_at.unwrap_or(0));
                    if newer {
                        best = Some((i, p));
                    }
                }
                Ok(_) => {}
            }
        }
        // Every source failed: say so instead of "nothing is playing".
        if errors == sources.len() {
            if let Some(e) = first_error {
                return Err(e);
            }
        }
        Ok((best.map(|(i, p)| (sources[i].0.clone(), p)), tabs))
    }

    pub async fn state(&self, want: Option<&str>) -> Result<StateView> {
        let (np, sources) = self.playing(want).await?;
        let entry = np.as_ref().and_then(|(a, p)| a.entry(&p.track));
        let playlists = self.store.view(entry.as_deref()).await?;
        let now_playing = np.map(|(a, p)| NowPlayingView {
            source: a.cfg.id.clone(),
            addable: entry.is_some(),
            progress: p.progress,
            id: p.track.id,
            name: p.track.title,
            artist: p.track.artist,
            album: p.track.album,
            album_id: p.track.album_id,
        });
        Ok(StateView { now_playing, playlists, sources })
    }

    /// Add the playing track to the playlist file, or remove it if it is already there.
    pub async fn toggle(self: &Arc<Self>, playlist_id: &str, want: Option<&str>) -> Result<StateView, ToggleError> {
        let (np, _) = self.playing(want).await?;
        let (active, playing) = np.ok_or(ToggleError::NothingPlaying)?;
        let entry = active.entry(&playing.track).ok_or(ToggleError::NotInLibrary)?;
        self.store.toggle(playlist_id, &entry).await?;
        self.tell_sources(vec![(playlist_id.to_string(), PlaylistChange::Edited)]);
        Ok(self.state(want).await?)
    }

    /// Best effort, in the background: every connected source hears about changed playlist files, each with the
    /// file's path as it sees it.
    fn tell_sources(&self, changes: Vec<(String, PlaylistChange)>) {
        for (a, s) in self.connected() {
            let events: Vec<PlaylistEvent> = changes
                .iter()
                .map(|(name, change)| PlaylistEvent { name: name.clone(), path: a.placement.playlist_path(name), change: *change })
                .collect();
            tokio::spawn(async move {
                if let Err(e) = s.playlists_changed(events).await {
                    tracing::warn!("source {}: could not pass on playlist changes: {e:#}", a.cfg.id);
                }
            });
        }
    }

    /// The cover of the album that holds the given track. `NotFound` when the source or track is unknown, the track
    /// is not in a library in use, or there is no cover. The ids are only lookup keys, never paths.
    pub async fn cover(&self, source_id: &str, id: &str, if_none_match: Option<&str>) -> crate::covers::Outcome {
        use crate::covers::{Outcome, resolve, valid_item_id};
        if !valid_item_id(id) {
            return Outcome::NotFound;
        }
        let Some((a, s)) = self.connected().into_iter().find(|(a, _)| a.cfg.id == source_id) else { return Outcome::NotFound };
        let Ok(track) = s.track(id).await else { return Outcome::NotFound };
        match a.entry(&track) {
            Some(entry) => resolve(&self.playlist_dir, &entry, if_none_match).await,
            None => Outcome::NotFound,
        }
    }

    // ----- Incoming -----

    /// Every enabled source with an incoming library (or only `want`), its albums newest first. Read-only.
    pub async fn incoming(&self, want: Option<&str>) -> IncomingView {
        let mut sources = Vec::new();
        for (a, s) in self.connected() {
            let Some(library) = a.cfg.libraries.incoming.clone() else { continue };
            if !a.cfg.enabled || want.is_some_and(|w| w != a.cfg.id) {
                continue;
            }
            let supported = s.lists_albums();
            let (albums, error) = if supported {
                match s.albums(&library, INCOMING_LIMIT).await {
                    Ok(list) => (list, None),
                    Err(e) => (Vec::new(), Some(format!("{e:#}"))),
                }
            } else {
                (Vec::new(), None)
            };
            sources.push(IncomingSourceView { id: a.cfg.id.clone(), label: label_of(&a.cfg), supported, albums, error });
        }
        IncomingView { sources }
    }

    /// Play an album of a source's incoming library on that source's player. Returns the player's label.
    pub async fn play_incoming(&self, source_id: &str, album_id: &str) -> Result<String> {
        anyhow::ensure!(crate::covers::valid_item_id(album_id), "not an album id");
        let (_, s) = self
            .connected()
            .into_iter()
            .find(|(a, _)| a.cfg.id == source_id && a.cfg.libraries.incoming.is_some())
            .ok_or_else(|| anyhow::anyhow!("no such source with an incoming library"))?;
        s.play_album(album_id).await
    }

    /// The cover of an album on a source that has an incoming library. The id is only passed on to the source.
    pub async fn incoming_cover(&self, source_id: &str, album_id: &str) -> Option<(String, Vec<u8>)> {
        if !crate::covers::valid_item_id(album_id) {
            return None;
        }
        let (_, s) = self.connected().into_iter().find(|(a, _)| a.cfg.id == source_id && a.cfg.libraries.incoming.is_some())?;
        match s.album_cover(album_id).await {
            Ok(found) => found,
            Err(e) => {
                tracing::warn!("cover of {album_id} on {source_id}: {e:#}");
                None
            }
        }
    }

    // ----- Settings -----

    fn kinds() -> Vec<KindView> {
        source::KINDS.iter().map(|k| KindView { id: k.id, label: k.label, fields: (k.fields)() }).collect()
    }

    /// Status, choices and a fresh look at the libraries. `source` is `None` when it could not be connected.
    async fn view_of(&self, cfg: &SourceConfig, source: Option<&dyn Source>, error: Option<String>) -> SourceView {
        let (values, secrets_set) = settings::redact(cfg);
        let mut view = SourceView {
            id: cfg.id.clone(),
            kind: cfg.kind.clone(),
            label: label_of(cfg),
            enabled: cfg.enabled,
            libraries: cfg.libraries.clone(),
            values,
            secrets_set,
            status: Status::default(),
            choices: BTreeMap::new(),
            placement: Placement::default(),
            error,
        };
        let Some(s) = source else { return view };
        let (status, choices, placed) = tokio::join!(s.status(), s.options(), placement::place(s, &self.playlist_dir));
        view.status = status.unwrap_or_default();
        match choices {
            Ok(c) => view.choices = c,
            Err(e) => {
                view.error.get_or_insert(format!("{e:#}"));
            }
        }
        match placed {
            Ok(p) => view.placement = p,
            Err(e) => {
                view.error.get_or_insert(format!("cannot list the libraries: {e:#}"));
            }
        }
        view
    }

    pub async fn settings(&self) -> Result<SettingsView> {
        let mut sources = Vec::new();
        for a in self.live().iter() {
            // A disabled source is shown with its choices too, so it can be checked before switching it on.
            let probe;
            let s: Option<&dyn Source> = match (&a.source, a.cfg.enabled) {
                (Some(s), _) => Some(&**s),
                (None, false) => {
                    probe = source::kind(&a.cfg.kind).and_then(|k| (k.connect)(&a.cfg.values, true).ok());
                    probe.as_deref()
                }
                (None, true) => None,
            };
            sources.push(self.view_of(&a.cfg, s, a.error.clone()).await);
        }
        let origin = *self.origin.read().unwrap_or_else(|p| p.into_inner());
        Ok(SettingsView {
            sources,
            kinds: Self::kinds(),
            origin,
            fixed: FixedSettings { bind: self.bind.clone(), playlist_dir: self.playlist_dir.display().to_string() },
            settings_file: self.settings_path.as_ref().map(|p| p.display().to_string()),
            playlists: self.playlist_count().await?,
            version: VERSION,
            started: self.started,
        })
    }

    /// One source as the page is filling it in: connect with what is there (a blank secret means the saved one,
    /// under the same rules as saving) and show its status, choices and libraries. Nothing is saved.
    pub async fn preview(&self, draft: SourceConfig) -> Result<SourceView, ToggleError> {
        let saved: Vec<SourceConfig> = self.live().iter().map(|a| a.cfg.clone()).collect();
        let cfg = settings::merge(&saved, vec![draft])?.remove(0);
        let kind = source::kind(&cfg.kind).ok_or_else(|| ToggleError::BadSettings("unknown kind of source".into()))?;
        let s = (kind.connect)(&cfg.values, true).map_err(|e| ToggleError::BadSettings(format!("{e:#}")))?;
        Ok(self.view_of(&cfg, Some(&*s), None).await)
    }

    /// Check every enabled source against its server, then save the list and switch over. Nothing is saved if a
    /// check fails.
    pub async fn save_settings(&self, drafts: Vec<SourceConfig>) -> Result<SettingsView, ToggleError> {
        let Some(path) = self.settings_path.clone() else { return Err(ToggleError::SettingsReadOnly) };
        let _one_at_a_time = self.saving.lock().await;
        let saved: Vec<SourceConfig> = self.live().iter().map(|a| a.cfg.clone()).collect();
        let merged = settings::merge(&saved, drafts)?;
        let mut next = Vec::with_capacity(merged.len());
        for cfg in merged.iter().cloned() {
            let a = Active::start(cfg, &self.playlist_dir).await;
            if a.cfg.enabled {
                let bad = |why: String| ToggleError::BadSettings(format!("{}: {why}", a.cfg.id));
                let s = a.source.as_ref().ok_or_else(|| bad(a.error.clone().unwrap_or_else(|| "cannot connect".into())))?;
                let st = s.status().await.map_err(|e| bad(format!("{e:#}")))?;
                if !st.connected {
                    return Err(bad(st.error.unwrap_or_else(|| "not connected".into())));
                }
                if a.placement.usable(&a.cfg.libraries).next().is_none() {
                    return Err(bad(format!(
                        "none of the chosen music libraries is inside the playlist folder ({})",
                        self.playlist_dir.display()
                    )));
                }
            }
            next.push(Arc::new(a));
        }
        tokio::task::spawn_blocking(move || settings::save(&path, &merged))
            .await
            .map_err(|e| anyhow::anyhow!("saving settings: {e}"))??;
        *self.live.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(next);
        *self.origin.write().unwrap_or_else(|p| p.into_inner()) = Origin::File;
        tracing::info!("settings saved and applied");
        drop(_one_at_a_time);
        Ok(self.settings().await?)
    }

    // ----- Playlist files -----

    pub async fn playlist_count(&self) -> Result<usize> {
        Ok(self.store.view(None).await?.len())
    }

    pub async fn repairs(&self) -> Result<Repairs> {
        self.store.repairs().await
    }

    /// Apply the repairs the client chose (`None` = every safe one). The store only accepts pairs it currently
    /// offers. The sources are then told about each changed playlist.
    pub async fn apply_repairs(self: &Arc<Self>, only: Option<Repair>) -> Result<Vec<String>, ToggleError> {
        let wanted = match only {
            Some(one) => vec![one],
            None => self.store.repairs().await?.safe,
        };
        let changed = self.store.apply_repairs(&wanted).await?;
        self.tell_sources(changed.iter().map(|n| (n.clone(), PlaylistChange::Edited)).collect());
        Ok(changed)
    }

    /// Put each playlist back as it was before its last change.
    pub async fn undo(self: &Arc<Self>, playlists: Vec<String>) -> Result<usize, ToggleError> {
        let mut done = Vec::new();
        for id in playlists {
            self.store.undo(&id).await?;
            done.push(id);
        }
        let n = done.len();
        self.tell_sources(done.into_iter().map(|n| (n, PlaylistChange::Edited)).collect());
        Ok(n)
    }

    /// Album folders that look like the same release twice, each group compared. Read-only. Track details come
    /// from the first source that knows the folder.
    pub async fn album_duplicates(&self) -> Result<Vec<crate::albums::AlbumGroup>> {
        use crate::albums::*;
        let music = self.playlist_dir.clone();
        let dir = music.clone();
        let folders: Vec<String> = tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
            Ok(std::fs::read_dir(&dir)?
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .collect())
        })
        .await??;
        let sources = self.connected();
        let mut out = Vec::new();
        for group in candidate_groups(&folders) {
            let mut albums = Vec::new();
            for folder in &group {
                let (m, f) = (music.clone(), folder.clone());
                let (files, cover) = tokio::task::spawn_blocking(move || read_folder(&m, &f)).await??;
                let mut known = Default::default();
                for (a, s) in &sources {
                    match s.album_details(folder).await {
                        Ok(k) if !k.is_empty() => {
                            known = k;
                            break;
                        }
                        Ok(_) => {}
                        Err(e) => tracing::warn!("source {} has no details for {folder:?}: {e:#}", a.cfg.id),
                    }
                }
                let lines = self.store.lines_in_folder(folder).await?;
                albums.push(build_view(folder, files, cover, &known, lines));
            }
            let comparisons = albums.iter().skip(1).map(|b| compare(&albums[0], b)).collect();
            let suggestion = suggest(&albums);
            out.push(AlbumGroup { albums, comparisons, suggestion });
        }
        Ok(out)
    }

    pub async fn duplicates(&self) -> Result<Vec<DupGroup>> {
        self.store.duplicates().await
    }

    /// Remove repeated entries from one playlist (the first copy of each stays).
    pub async fn dedupe(self: &Arc<Self>, id: &str) -> Result<usize, ToggleError> {
        let removed = self.store.dedupe(id).await?;
        if removed > 0 {
            self.tell_sources(vec![(id.to_string(), PlaylistChange::Edited)]);
        }
        Ok(removed)
    }

    pub async fn create_playlist(self: &Arc<Self>, name: &str) -> Result<(), ToggleError> {
        self.store.create(name).await?;
        self.tell_sources(vec![(name.to_string(), PlaylistChange::Created)]);
        Ok(())
    }

    pub async fn rename_playlist(self: &Arc<Self>, from: &str, to: &str) -> Result<(), ToggleError> {
        self.store.rename(from, to).await?;
        self.tell_sources(vec![(from.to_string(), PlaylistChange::Deleted), (to.to_string(), PlaylistChange::Created)]);
        Ok(())
    }

    pub async fn delete_playlist(self: &Arc<Self>, id: &str) -> Result<(), ToggleError> {
        self.store.delete(id).await?;
        self.tell_sources(vec![(id.to_string(), PlaylistChange::Deleted)]);
        Ok(())
    }

    pub async fn restore_playlist(self: &Arc<Self>, id: &str) -> Result<(), ToggleError> {
        self.store.restore_deleted(id).await?;
        self.tell_sources(vec![(id.to_string(), PlaylistChange::Created)]);
        Ok(())
    }

    /// Re-check which playlist entries point at missing files (the library changes without the playlists).
    pub async fn refresh_dead(&self) {
        self.store.refresh_dead().await;
    }
}
