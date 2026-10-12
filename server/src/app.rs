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
use crate::settings::{self, ChangeError, Folders, Origin, SourceConfig};
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
    /// The player it plays on ("Finamp on Lenovo Tab"); the controls act on it.
    pub player: Option<String>,
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
    #[error("{0}")]
    CannotPlay(String),
    #[error("no Music library is set")]
    NoMusicFolder,
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

/// Each album already in the library, by album id: an upgrade of it or a duplicate. The album's release group comes
/// from the catalogue, matched by the end of its folder path (the server's paths and Cratarr's differ in front).
fn verdicts(
    albums: &[source::Album],
    library: &crate::incoming::LibraryIndex,
    incoming: &[(String, Option<String>)],
) -> BTreeMap<String, VerdictView> {
    use crate::incoming::{AlbumFacts, Verdict, quality_of};
    let mut out = BTreeMap::new();
    for a in albums {
        let release_group = a.path.as_deref().and_then(|p| {
            incoming.iter().find(|(dir, _)| !dir.is_empty() && p.ends_with(&format!("/{dir}"))).and_then(|(_, rg)| rg.clone())
        });
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

/// How long the library index (for Incoming's verdicts) is kept before it is read from the catalogue again.
const INDEX_SECS: i64 = 300;

/// The library index plus the incoming albums' release groups by folder, read together from the catalogue.
struct Owned {
    built: i64,
    library: Arc<crate::incoming::LibraryIndex>,
    /// (album directory under an Incoming folder, its release group).
    incoming: Arc<Vec<(String, Option<String>)>>,
}

#[derive(Debug, Serialize)]
pub struct IncomingView {
    pub sources: Vec<IncomingSourceView>,
}

/// How often the background pass runs (see `App::sync`).
pub const SYNC_SECS: u64 = 30;

/// More than any incoming queue should hold; the page shows them all.
const INCOMING_LIMIT: usize = 1000;

#[derive(Debug, Serialize)]
pub struct StatusView {
    pub connected: bool,
    /// A Music library is set and its folder found: playlists can be read and made.
    pub music: bool,
    /// Some enabled server has an Incoming library: the page shows incoming-music features.
    pub incoming: bool,
    /// Connected sources, for the source picker (`playing` is not checked here and always false).
    pub sources: Vec<SourceTab>,
    /// The server's name with one source; "n of m sources" with several.
    pub server_name: Option<String>,
    pub version: &'static str,
}

/// Set by the container, so shown but not editable.
#[derive(Debug, Serialize)]
pub struct FixedSettings {
    pub bind: String,
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
    /// Secrets left out; see `secrets_set`.
    pub values: source::Values,
    pub secrets_set: Vec<String>,
    pub status: Status,
    pub choices: BTreeMap<String, Vec<Choice>>,
    /// What its libraries are used for, as saved.
    pub libraries: settings::Roles,
    /// Its music libraries and where Cratarr sees their files (decision 0009).
    pub server_libraries: Vec<placement::PlacedLibrary>,
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
    /// Where the Music libraries' files are, as Cratarr sees them: the playlists live there.
    pub music_folder: Option<String>,
    pub tag_rules: crate::catalogue::Rules,
    pub catalogue: Option<crate::catalogue::Status>,
}

/// The catalogue's folders: the Music folder first (id 0), then the incoming ones.
fn catalogue_folders(folders: &Folders) -> Vec<PathBuf> {
    folders.all().into_iter().map(PathBuf::from).collect()
}

/// The folders mounted into the container, without the data folder.
fn mounted(data_dir: Option<&str>) -> Vec<String> {
    crate::mounts::list(&data_dir.into_iter().collect::<Vec<_>>())
}

/// Every folder a server's music is looked for in: the mounted ones, and the Music folder in use (which need not be a
/// mount point of its own).
fn place_folders(data_dir: Option<&str>, music: Option<&str>) -> Vec<String> {
    let mut out = mounted(data_dir);
    if let Some(m) = music {
        if !out.iter().any(|f| f == m) {
            out.push(m.to_string());
        }
    }
    out
}

/// Where the libraries in use keep their files, as Cratarr sees them (decision 0009). The Music folder is the first
/// one a Music library was found in; `last_music` stands in while no server has answered (or none has a Music
/// library yet).
fn derive_folders(live: &[Arc<Active>], last_music: Option<&str>) -> Folders {
    let found = |a: &Active, ids: &[String]| -> Vec<String> {
        ids.iter()
            .filter_map(|id| a.placement.libraries.iter().find(|l| &l.id == id))
            .flat_map(|l| l.folders.clone())
            .collect()
    };
    let up: Vec<&Arc<Active>> = live.iter().filter(|a| a.source.is_some()).collect();
    let music = up.iter().flat_map(|a| found(a, &a.cfg.libraries.music)).next().or_else(|| last_music.map(str::to_string));
    let mut incoming: Vec<String> =
        up.iter().flat_map(|a| found(a, &a.cfg.libraries.incoming)).filter(|f| Some(f) != music.as_ref()).collect();
    incoming.sort();
    incoming.dedup();
    Folders { music, incoming }
}

/// The most tracks one play sends: Finamp holds at most 100 in its queue (seen 2026-10-12; user: "we go by 100").
pub const PLAY_MAX: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayMode {
    Play,
    Shuffle,
    Next,
    Last,
}

#[derive(Debug, Serialize)]
pub struct PlayResult {
    /// "Finamp on Lenovo Tab".
    pub player: String,
    /// Tracks sent (at most PLAY_MAX).
    pub sent: usize,
    /// Playable tracks in the playlist.
    pub total: usize,
    /// Lines whose track is missing or not known to the server yet; left out.
    pub unknown: usize,
}

/// Shuffle in place (Fisher–Yates) with a small generator seeded from the clock; good enough for a playlist.
fn shuffle<T>(items: &mut [T]) {
    let mut x = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) | 1;
    for i in (1..items.len()).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        items.swap(i, (x % (i as u64 + 1)) as usize);
    }
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
    async fn start(cfg: SourceConfig, folders: &[String]) -> Self {
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
        let placement = placement::place(&*source, folders).await.unwrap_or_else(|e| {
            tracing::warn!("source {}: cannot look at its libraries yet: {e:#}", cfg.id);
            Placement::default()
        });
        if placement.roots.is_empty() {
            tracing::warn!("source {}: none of its music found in Cratarr's folders (yet)", cfg.id);
        }
        Self { cfg, source: Some(source), placement, error: None }
    }

    /// The playlist line for a track: its path in the Music folder, if it lies there.
    fn entry(&self, music: Option<&str>, track: &source::Track) -> Option<String> {
        self.placement.entry(music?, track.path.as_deref())
    }
}

pub struct App {
    live: std::sync::RwLock<Arc<Vec<Arc<Active>>>>,
    origin: std::sync::RwLock<Origin>,
    bind: String,
    data_dir: Option<String>,
    settings_path: Option<PathBuf>,
    /// One settings change at a time, so two pages (or the background placement retry) cannot interleave.
    saving: Mutex<()>,
    started: i64,
    /// Where the libraries in use keep their files, and the playlists of the Music folder (swapped in whole when
    /// either changes).
    folders: std::sync::RwLock<(Folders, Option<Arc<Store>>)>,
    /// Incoming lists by source and library (see `refresh_incoming`).
    incoming_cache: Mutex<std::collections::HashMap<String, IncomingSourceView>>,
    /// See `Owned`; `None` until first needed, and after settings change.
    owned: Mutex<Option<Owned>>,
    /// Every audio file Cratarr can see, and the servers' ids for them (decision 0008).
    catalogue: Arc<crate::catalogue::Catalogue>,
    /// Saved with the sources.
    tag_rules: std::sync::RwLock<crate::catalogue::Rules>,
}

impl App {
    pub async fn new(cfg: Config, saved: settings::Saved, origin: Origin) -> Result<Arc<Self>> {
        let last_music = saved.music_folder.or_else(|| cfg.playlist_dir.clone());
        let places = place_folders(cfg.data_dir.as_deref(), last_music.as_deref());
        let mut active = Vec::new();
        for s in saved.sources {
            active.push(Active::start(s, &places).await);
        }
        // First run: no Music library chosen yet, so the libraries found in CRATARR_PLAYLIST_DIR are Music (shown so on
        // the Settings page, kept once saved).
        if let Some(m) = last_music.as_deref().filter(|_| active.iter().all(|a| a.cfg.libraries.music.is_empty())) {
            for a in active.iter_mut().filter(|a| a.source.is_some()) {
                let incoming = a.cfg.libraries.incoming.clone();
                a.cfg.libraries.music =
                    a.placement.libraries_in(m).map(|l| l.id.clone()).filter(|id| !incoming.contains(id)).collect();
            }
        }
        let active: Vec<Arc<Active>> = active.into_iter().map(Arc::new).collect();
        let folders = derive_folders(&active, last_music.as_deref());
        let db = cfg.data_dir.as_ref().map(|d| PathBuf::from(d).join("catalogue.sqlite"));
        let catalogue = Arc::new(crate::catalogue::Catalogue::open(db.as_deref(), catalogue_folders(&folders))?);
        let store = folders.music.as_ref().map(|m| Arc::new(Store::new(m)));
        Ok(Arc::new(Self {
            live: std::sync::RwLock::new(Arc::new(active)),
            origin: std::sync::RwLock::new(origin),
            settings_path: cfg.data_dir.as_ref().map(|d| PathBuf::from(d).join("settings.json")),
            bind: cfg.bind,
            data_dir: cfg.data_dir,
            saving: Mutex::new(()),
            started: epoch_now(),
            folders: std::sync::RwLock::new((folders, store)),
            incoming_cache: Mutex::new(std::collections::HashMap::new()),
            owned: Mutex::new(None),
            catalogue,
            tag_rules: std::sync::RwLock::new(saved.tag_rules),
        }))
    }

    fn folders(&self) -> Folders {
        self.folders.read().unwrap_or_else(|p| p.into_inner()).0.clone()
    }

    fn music(&self) -> Option<String> {
        self.folders.read().unwrap_or_else(|p| p.into_inner()).0.music.clone()
    }

    /// The Music folder's playlists, if one is set.
    fn store_opt(&self) -> Option<Arc<Store>> {
        self.folders.read().unwrap_or_else(|p| p.into_inner()).1.clone()
    }

    fn store(&self) -> Result<Arc<Store>, ToggleError> {
        self.store_opt().ok_or(ToggleError::NoMusicFolder)
    }

    /// Put new sources in use, and follow where their libraries' files are: a new Music folder gets its own playlist
    /// store, and the catalogue follows the folders.
    async fn switch_to(&self, next: Vec<Arc<Active>>) -> Result<()> {
        let old = self.folders();
        let folders = derive_folders(&next, old.music.as_deref());
        *self.live.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(next);
        if folders != old {
            tracing::info!("music folder {:?}, incoming folders {:?}", folders.music, folders.incoming);
            let (c, dirs) = (self.catalogue.clone(), catalogue_folders(&folders));
            tokio::task::spawn_blocking(move || c.set_folders(dirs)).await.map_err(|e| anyhow::anyhow!("{e}"))??;
            let mut slot = self.folders.write().unwrap_or_else(|p| p.into_inner());
            if folders.music != old.music {
                slot.1 = folders.music.as_ref().map(|m| Arc::new(Store::new(m)));
            }
            slot.0 = folders;
        }
        Ok(())
    }

    /// Play a playlist on the server's player: its tracks in file order (or shuffled), looked up in the catalogue,
    /// at most PLAY_MAX of them. `want` = the server to use (default: the first connected one).
    pub async fn play_playlist(&self, id: &str, mode: PlayMode, want: Option<&str>) -> Result<PlayResult, ToggleError> {
        let entries = self.store()?.entries(id).await?;
        let sources = self.connected();
        let (active, source) = want
            .and_then(|w| sources.iter().find(|(a, _)| a.cfg.id == w))
            .or_else(|| sources.first())
            .cloned()
            .ok_or_else(|| ToggleError::CannotPlay("no server is connected".into()))?;
        let (c, sid) = (self.catalogue.clone(), active.cfg.id.clone());
        let found = tokio::task::spawn_blocking(move || c.track_ids(&sid, 0, &entries))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))??;
        let unknown = found.iter().filter(|f| f.is_none()).count();
        let mut ids: Vec<String> = found.into_iter().flatten().collect();
        let total = ids.len();
        if total == 0 {
            // Usually the catalogue is still matching files to the server (a first fill takes minutes): say so.
            let c = self.catalogue.clone();
            let status = tokio::task::spawn_blocking(move || c.status()).await.ok().and_then(Result::ok);
            let why = match status {
                Some(st) if st.linked.get(&active.cfg.id).copied().unwrap_or(0) < st.files => format!(
                    "Cratarr is still matching your music with {} ({} of {} tracks); try again in a few minutes",
                    label_of(&active.cfg),
                    st.linked.get(&active.cfg.id).copied().unwrap_or(0),
                    st.files
                ),
                _ => format!("none of this playlist's tracks are on {}", label_of(&active.cfg)),
            };
            return Err(ToggleError::CannotPlay(why));
        }
        if mode == PlayMode::Shuffle {
            shuffle(&mut ids);
        }
        ids.truncate(PLAY_MAX);
        let how = match mode {
            PlayMode::Play | PlayMode::Shuffle => source::PlayHow::Now,
            PlayMode::Next => source::PlayHow::Next,
            PlayMode::Last => source::PlayHow::Last,
        };
        let player = source.play_tracks(&ids, how).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))?;
        Ok(PlayResult { player, sent: ids.len(), total, unknown })
    }

    pub fn catalogue(&self) -> Arc<crate::catalogue::Catalogue> {
        self.catalogue.clone()
    }

    /// The connected servers by id, for the catalogue's background pass.
    pub fn connected_sources(&self) -> Vec<(String, Arc<dyn Source>)> {
        self.connected().into_iter().map(|(a, s)| (a.cfg.id.clone(), s)).collect()
    }

    fn live(&self) -> Arc<Vec<Arc<Active>>> {
        self.live.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    fn connected(&self) -> Vec<(Arc<Active>, Arc<dyn Source>)> {
        self.live().iter().filter_map(|a| a.source.clone().map(|s| (a.clone(), s))).collect()
    }

    /// Servers whose libraries could not be listed, or whose Music library's files were not found, get looked at
    /// again (called now and then): they may have been down, or still scanning. An Incoming library Cratarr cannot see
    /// is not retried: only a new mount (a restart) changes that.
    pub async fn retry_placement(&self) {
        let _one_at_a_time = self.saving.lock().await;
        let live = self.live();
        let unplaced = |a: &Active| {
            a.source.is_some()
                && (a.placement.libraries.is_empty()
                    || a.cfg.libraries.music.iter().any(|id| {
                        a.placement.libraries.iter().find(|l| &l.id == id).is_none_or(|l| l.folders.is_empty())
                    }))
        };
        if !live.iter().any(|a| unplaced(a)) {
            return;
        }
        let places = place_folders(self.data_dir.as_deref(), self.music().as_deref());
        let mut next = Vec::with_capacity(live.len());
        for a in live.iter() {
            match &a.source {
                Some(s) if unplaced(a) => {
                    let placement = placement::place(&**s, &places).await.unwrap_or_default();
                    if placement.roots.len() > a.placement.roots.len() {
                        tracing::info!("source {}: found its music in more folders", a.cfg.id);
                    }
                    next.push(Arc::new(Active { cfg: a.cfg.clone(), source: Some(s.clone()), placement, error: a.error.clone() }));
                }
                _ => next.push(a.clone()),
            }
        }
        if let Err(e) = self.switch_to(next).await {
            tracing::warn!("cannot follow the music folders: {e:#}");
        }
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
        let folders = self.folders();
        let sources = sources.iter().map(|(a, _)| SourceTab { id: a.cfg.id.clone(), label: label_of(&a.cfg), playing: false }).collect();
        StatusView {
            connected: up > 0,
            music: folders.music.is_some(),
            incoming: self.live().iter().any(|a| a.cfg.enabled && !a.cfg.libraries.incoming.is_empty()),
            sources,
            server_name,
            version: VERSION,
        }
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

    /// Previous / play-pause / next on the player that plays now (on `want`'s server, else the one playing).
    pub async fn transport(&self, want: Option<&str>, command: source::Transport) -> Result<String, ToggleError> {
        let (np, _) = self.playing(want).await?;
        let source = match np {
            Some((a, _)) => self.connected().into_iter().find(|(c, _)| c.cfg.id == a.cfg.id).map(|(_, s)| s),
            None => None,
        }
        .ok_or_else(|| ToggleError::CannotPlay("nothing is playing".into()))?;
        source.transport(command).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
    }

    pub async fn state(&self, want: Option<&str>) -> Result<StateView> {
        let (np, sources) = self.playing(want).await?;
        let music = self.music();
        let entry = np.as_ref().and_then(|(a, p)| a.entry(music.as_deref(), &p.track));
        let playlists = match self.store_opt() {
            Some(store) => store.view(entry.as_deref()).await?,
            None => Vec::new(),
        };
        let now_playing = np.map(|(a, p)| NowPlayingView {
            source: a.cfg.id.clone(),
            addable: entry.is_some(),
            progress: p.progress,
            id: p.track.id,
            name: p.track.title,
            artist: p.track.artist,
            album: p.track.album,
            album_id: p.track.album_id,
            player: p.player,
        });
        Ok(StateView { now_playing, playlists, sources })
    }

    /// Add the playing track to the playlist file, or remove it if it is already there.
    pub async fn toggle(self: &Arc<Self>, playlist_id: &str, want: Option<&str>) -> Result<StateView, ToggleError> {
        let (np, _) = self.playing(want).await?;
        let (active, playing) = np.ok_or(ToggleError::NothingPlaying)?;
        let store = self.store()?;
        let entry = active.entry(self.music().as_deref(), &playing.track).ok_or(ToggleError::NotInLibrary)?;
        store.toggle(playlist_id, &entry).await?;
        self.tell_sources(vec![(playlist_id.to_string(), PlaylistChange::Edited)]);
        Ok(self.state(want).await?)
    }

    /// Best effort, in the background: every connected source hears about changed playlist files, each with the
    /// file's path as it sees it.
    fn tell_sources(&self, changes: Vec<(String, PlaylistChange)>) {
        let Some(music) = self.music() else { return };
        for (a, s) in self.connected() {
            let events: Vec<PlaylistEvent> = changes
                .iter()
                .map(|(name, change)| PlaylistEvent { name: name.clone(), path: a.placement.playlist_path(&music, name), change: *change })
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
        let Some(music) = self.music() else { return Outcome::NotFound };
        match a.entry(Some(&music), &track) {
            Some(entry) => resolve(std::path::Path::new(&music), &entry, if_none_match).await,
            None => Outcome::NotFound,
        }
    }

    // ----- Incoming -----

    /// Every server with Incoming libraries (or only `want`), their albums newest first. Read-only. Only the server is
    /// asked: the list works whether or not Cratarr can see the files.
    /// The kept lists, read from the server only when there is none yet (the background refresh keeps them fresh).
    pub async fn incoming(&self, want: Option<&str>) -> IncomingView {
        let mut sources = Vec::new();
        for (a, s, libraries) in self.incoming_sources() {
            if want.is_some_and(|w| w != a.cfg.id) {
                continue;
            }
            let key = format!("{}\n{}", a.cfg.id, libraries.join("\n"));
            let kept = self.incoming_cache.lock().await.get(&key).cloned();
            let view = match kept {
                Some(v) => v,
                None => {
                    let v = Self::read_incoming(&a, &*s, &libraries).await;
                    self.incoming_cache.lock().await.insert(key, v.clone());
                    v
                }
            };
            sources.push(view);
        }
        if let Some((library, incoming)) = self.owned_index().await {
            for view in &mut sources {
                view.verdicts = verdicts(&view.albums, &library, &incoming);
            }
        }
        IncomingView { sources }
    }

    /// The library index and the incoming release groups, read from the catalogue at most every `INDEX_SECS`.
    async fn owned_index(&self) -> Option<(Arc<crate::incoming::LibraryIndex>, Arc<Vec<(String, Option<String>)>>)> {
        let mut owned = self.owned.lock().await;
        if let Some(o) = owned.as_ref().filter(|o| epoch_now() - o.built < INDEX_SECS) {
            return Some((o.library.clone(), o.incoming.clone()));
        }
        let c = self.catalogue.clone();
        let read = tokio::task::spawn_blocking(move || -> Result<(crate::incoming::LibraryIndex, Vec<(String, Option<String>)>)> {
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
                incoming.extend(c.album_facts(id)?.into_iter().map(|f| (f.dir, f.release_group)));
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
    pub async fn refresh_incoming(&self) {
        let mut keep = std::collections::HashSet::new();
        for (a, s, libraries) in self.incoming_sources() {
            let key = format!("{}\n{}", a.cfg.id, libraries.join("\n"));
            let fresh = Self::read_incoming(&a, &*s, &libraries).await;
            let mut cache = self.incoming_cache.lock().await;
            if fresh.error.is_none() || !cache.contains_key(&key) {
                cache.insert(key.clone(), fresh);
            }
            keep.insert(key);
        }
        self.incoming_cache.lock().await.retain(|k, _| keep.contains(k));
    }

    /// Each connected server with Incoming libraries, and their ids.
    fn incoming_sources(&self) -> Vec<(Arc<Active>, Arc<dyn Source>, Vec<String>)> {
        self.connected()
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
        IncomingSourceView { id: a.cfg.id.clone(), label: label_of(&a.cfg), supported, albums, error, as_of: epoch_now(), verdicts: BTreeMap::new() }
    }

    /// The one background pass that keeps Cratarr in step with its sources and the library, run every
    /// `SYNC_SECS`. Sources offer no reliable change notifications (webhooks are optional plugins), so it looks:
    /// incoming lists every pass; dead playlist entries and sources still missing their libraries every other pass.
    pub async fn sync(&self, pass: u64) {
        self.refresh_incoming().await;
        if pass % 2 == 0 {
            self.refresh_dead().await;
            self.retry_placement().await;
        }
    }

    /// Play an album of a server's incoming music on that server's player. Returns the player's label.
    pub async fn play_incoming(&self, source_id: &str, album_id: &str) -> Result<String> {
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
    pub async fn incoming_cover(&self, source_id: &str, album_id: &str) -> Option<(String, Vec<u8>)> {
        if !crate::covers::valid_item_id(album_id) {
            return None;
        }
        let (_, s) = self.connected().into_iter().find(|(a, _)| a.cfg.id == source_id)?;
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

    /// Status, choices and a fresh look at where its libraries lie. `source` is `None` when it could not be
    /// connected.
    async fn view_of(&self, cfg: &SourceConfig, source: Option<&dyn Source>, error: Option<String>) -> SourceView {
        let (values, secrets_set) = settings::redact(cfg);
        let mut view = SourceView {
            id: cfg.id.clone(),
            kind: cfg.kind.clone(),
            label: label_of(cfg),
            enabled: cfg.enabled,
            values,
            secrets_set,
            status: Status::default(),
            choices: BTreeMap::new(),
            libraries: cfg.libraries.clone(),
            server_libraries: Vec::new(),
            error,
        };
        let Some(s) = source else { return view };
        let places = place_folders(self.data_dir.as_deref(), self.music().as_deref());
        let (status, choices, placed) = tokio::join!(s.status(), s.options(), placement::place(s, &places));
        view.status = status.unwrap_or_default();
        match choices {
            Ok(c) => view.choices = c,
            Err(e) => {
                view.error.get_or_insert(format!("{e:#}"));
            }
        }
        match placed {
            Ok(p) => view.server_libraries = p.libraries,
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
        // Read before the struct below: a lock guard must not live across its awaits.
        let tag_rules = self.tag_rules.read().unwrap_or_else(|p| p.into_inner()).clone();
        let music_folder = self.music();
        Ok(SettingsView {
            sources,
            kinds: Self::kinds(),
            origin,
            fixed: FixedSettings { bind: self.bind.clone() },
            settings_file: self.settings_path.as_ref().map(|p| p.display().to_string()),
            playlists: self.playlist_count().await?,
            version: VERSION,
            started: self.started,
            music_folder,
            tag_rules,
            catalogue: {
                let c = self.catalogue.clone();
                tokio::task::spawn_blocking(move || c.status()).await.ok().and_then(Result::ok)
            },
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
    /// check fails. The Music libraries' files hold the playlists, so Cratarr must see them, all in one folder.
    /// `tag_rules`: `None` keeps what is saved.
    pub async fn save_settings(
        &self,
        drafts: Vec<SourceConfig>,
        tag_rules: Option<crate::catalogue::Rules>,
    ) -> Result<SettingsView, ToggleError> {
        let Some(path) = self.settings_path.clone() else { return Err(ToggleError::SettingsReadOnly) };
        let _one_at_a_time = self.saving.lock().await;
        let old_rules = self.tag_rules.read().unwrap_or_else(|p| p.into_inner()).clone();
        let places = place_folders(self.data_dir.as_deref(), self.music().as_deref());
        let tag_rules = tag_rules.unwrap_or(old_rules);
        let saved: Vec<SourceConfig> = self.live().iter().map(|a| a.cfg.clone()).collect();
        let merged = settings::merge(&saved, drafts)?;
        let mut next = Vec::with_capacity(merged.len());
        for cfg in merged.iter().cloned() {
            let a = Active::start(cfg, &places).await;
            if a.cfg.enabled {
                let bad = |why: String| ToggleError::BadSettings(format!("{}: {why}", a.cfg.id));
                let s = a.source.as_ref().ok_or_else(|| bad(a.error.clone().unwrap_or_else(|| "cannot connect".into())))?;
                let st = s.status().await.map_err(|e| bad(format!("{e:#}")))?;
                if !st.connected {
                    return Err(bad(st.error.unwrap_or_else(|| "not connected".into())));
                }
                for id in &a.cfg.libraries.music {
                    match a.placement.libraries.iter().find(|l| &l.id == id) {
                        None => return Err(bad("a Music library is no longer on the server".into())),
                        Some(l) if l.folders.is_empty() => {
                            return Err(ToggleError::BadSettings(format!("Cratarr can't see the files of {}", l.name)));
                        }
                        Some(_) => {}
                    }
                }
            }
            next.push(Arc::new(a));
        }
        let mut music_folders: Vec<String> = next
            .iter()
            .filter(|a| a.source.is_some())
            .flat_map(|a| {
                a.cfg.libraries.music.iter().filter_map(|id| a.placement.libraries.iter().find(|l| &l.id == id)?.folders.first().cloned())
            })
            .collect();
        music_folders.sort();
        music_folders.dedup();
        if music_folders.len() > 1 {
            return Err(ToggleError::BadSettings(format!(
                "the Music libraries are in different folders ({}); playlists live in one",
                music_folders.join(", ")
            )));
        }
        let music_folder = derive_folders(&next, self.music().as_deref()).music;
        let to_save = settings::Saved { sources: merged, music_folder, tag_rules: tag_rules.clone() };
        tokio::task::spawn_blocking(move || settings::save(&path, &to_save))
            .await
            .map_err(|e| anyhow::anyhow!("saving settings: {e}"))??;
        *self.tag_rules.write().unwrap_or_else(|p| p.into_inner()) = tag_rules;
        self.switch_to(next).await?;
        // Servers may have changed: link their tracks again soon.
        self.catalogue.poke();
        *self.origin.write().unwrap_or_else(|p| p.into_inner()) = Origin::File;
        // Sources and libraries may have changed: lists are read again on the next look (or the next sync pass).
        self.incoming_cache.lock().await.clear();
        *self.owned.lock().await = None;
        tracing::info!("settings saved and applied");
        drop(_one_at_a_time);
        Ok(self.settings().await?)
    }

    // ----- Playlist files -----

    /// 0 when there is no Music folder.
    pub async fn playlist_count(&self) -> Result<usize> {
        match self.store_opt() {
            Some(store) => Ok(store.view(None).await?.len()),
            None => Ok(0),
        }
    }

    pub async fn repairs(&self) -> Result<Repairs> {
        self.store()?.repairs().await
    }

    /// Apply the repairs the client chose (`None` = every safe one). The store only accepts pairs it currently
    /// offers. The sources are then told about each changed playlist.
    pub async fn apply_repairs(self: &Arc<Self>, only: Option<Repair>) -> Result<Vec<String>, ToggleError> {
        let wanted = match only {
            Some(one) => vec![one],
            None => self.store()?.repairs().await?.safe,
        };
        let changed = self.store()?.apply_repairs(&wanted).await?;
        self.tell_sources(changed.iter().map(|n| (n.clone(), PlaylistChange::Edited)).collect());
        Ok(changed)
    }

    /// Put each playlist back as it was before its last change.
    pub async fn undo(self: &Arc<Self>, playlists: Vec<String>) -> Result<usize, ToggleError> {
        let mut done = Vec::new();
        for id in playlists {
            self.store()?.undo(&id).await?;
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
        let Some(music) = self.music().map(PathBuf::from) else { return Ok(Vec::new()) };
        let store = self.store()?;
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
                let lines = store.lines_in_folder(folder).await?;
                albums.push(build_view(folder, files, cover, &known, lines));
            }
            let comparisons = albums.iter().skip(1).map(|b| compare(&albums[0], b)).collect();
            let suggestion = suggest(&albums);
            out.push(AlbumGroup { albums, comparisons, suggestion });
        }
        Ok(out)
    }

    pub async fn duplicates(&self) -> Result<Vec<DupGroup>> {
        self.store()?.duplicates().await
    }

    /// Remove repeated entries from one playlist (the first copy of each stays).
    pub async fn dedupe(self: &Arc<Self>, id: &str) -> Result<usize, ToggleError> {
        let removed = self.store()?.dedupe(id).await?;
        if removed > 0 {
            self.tell_sources(vec![(id.to_string(), PlaylistChange::Edited)]);
        }
        Ok(removed)
    }

    pub async fn create_playlist(self: &Arc<Self>, name: &str) -> Result<(), ToggleError> {
        self.store()?.create(name).await?;
        self.tell_sources(vec![(name.to_string(), PlaylistChange::Created)]);
        Ok(())
    }

    pub async fn rename_playlist(self: &Arc<Self>, from: &str, to: &str) -> Result<(), ToggleError> {
        self.store()?.rename(from, to).await?;
        self.tell_sources(vec![(from.to_string(), PlaylistChange::Deleted), (to.to_string(), PlaylistChange::Created)]);
        Ok(())
    }

    pub async fn delete_playlist(self: &Arc<Self>, id: &str) -> Result<(), ToggleError> {
        self.store()?.delete(id).await?;
        self.tell_sources(vec![(id.to_string(), PlaylistChange::Deleted)]);
        Ok(())
    }

    pub async fn restore_playlist(self: &Arc<Self>, id: &str) -> Result<(), ToggleError> {
        self.store()?.restore_deleted(id).await?;
        self.tell_sources(vec![(id.to_string(), PlaylistChange::Created)]);
        Ok(())
    }

    /// Re-check which playlist entries point at missing files (the library changes without the playlists).
    pub async fn refresh_dead(&self) {
        if let Some(store) = self.store_opt() {
            store.refresh_dead().await;
        }
    }
}
