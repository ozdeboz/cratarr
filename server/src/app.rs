//! Application logic: works out what is playing and which playlist files contain it, and decides what a tap does.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;
use tokio::sync::Mutex;

use crate::config::Config;
use crate::jellyfin::{Jellyfin, PlayingItem, User, pick_now_playing};
use crate::m3u::library_entry;
use crate::nowplaying::{Progress, current_from_activity, has_audio_history, progress};
use crate::settings::{self, Change, ChangeError, Overrides, Source};
use crate::playlists::{DupGroup, PlaylistView, Repair, Repairs, Store, StoreError};

#[derive(Debug, Serialize, PartialEq)]
pub struct NowPlayingView {
    pub id: String,
    pub name: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// False when the track is outside the curated library (an incoming folder, say): it cannot be added.
    pub addable: bool,
    /// `None` when nothing reports a position (no matching session and no start time).
    pub progress: Option<Progress>,
}

#[derive(Debug, Serialize)]
pub struct StateView {
    pub now_playing: Option<NowPlayingView>,
    pub playlists: Vec<PlaylistView>,
}

#[derive(Debug, thiserror::Error)]
pub enum ToggleError {
    #[error("nothing is playing")]
    NothingPlaying,
    #[error("the playing track is not in the curated library, so it cannot be added to a playlist")]
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

#[derive(Debug, Serialize)]
pub struct StatusView {
    pub connected: bool,
    pub server_name: Option<String>,
    pub version: &'static str,
}

#[derive(Debug, Serialize)]
pub struct SettingsValues {
    pub jellyfin_url: String,
    /// The key itself never leaves the server.
    pub api_key_set: bool,
    pub user_id: String,
    pub player_client: String,
    pub music_prefixes: Vec<String>,
    pub jellyfin_playlist_dir: String,
}

/// Set by the container (port, mount), so shown but not editable.
#[derive(Debug, Serialize)]
pub struct FixedSettings {
    pub bind: String,
    pub playlist_dir: String,
}

#[derive(Debug, Serialize)]
pub struct JellyfinStatus {
    pub ok: bool,
    pub server_name: Option<String>,
    pub version: Option<String>,
    pub user_name: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SettingsView {
    pub values: SettingsValues,
    pub sources: std::collections::BTreeMap<&'static str, Source>,
    pub fixed: FixedSettings,
    /// `None` when CRATARR_DATA_DIR is unset: nothing can be saved.
    pub settings_file: Option<String>,
    pub jellyfin: JellyfinStatus,
    pub playlists: usize,
    pub version: &'static str,
    /// Epoch seconds.
    pub started: i64,
}

#[derive(Debug, Serialize)]
pub struct ProbeView {
    pub server_name: Option<String>,
    pub version: String,
    pub users: Vec<User>,
}

/// A track that started longer ago than this without a stop entry is assumed to be a stale log line.
const MAX_PLAYING_AGE_SECS: i64 = 3 * 3600;

fn epoch_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// What the Settings page can swap while running. Read through `cfg()` and `jf()`, which clone, so the lock is
/// never held across an await.
struct Live {
    cfg: Arc<Config>,
    jf: Jellyfin,
    overrides: Overrides,
}

/// Where page-saved settings live (`path: None`: the page can look but not save).
pub struct SettingsFile {
    pub path: Option<std::path::PathBuf>,
    pub overrides: Overrides,
}

pub struct App {
    live: std::sync::RwLock<Live>,
    settings_path: Option<std::path::PathBuf>,
    /// One save at a time, so two pages cannot interleave check-then-write.
    saving: Mutex<()>,
    started: i64,
    store: Store,
    /// Details of recently seen tracks, so polling the playing track costs one cheap request.
    items: Mutex<HashMap<String, PlayingItem>>,
}

impl App {
    pub fn new(cfg: Config, jf: Jellyfin, settings: SettingsFile) -> Arc<Self> {
        let store = Store::new(&cfg.playlist_dir);
        Arc::new(Self {
            live: std::sync::RwLock::new(Live { cfg: Arc::new(cfg), jf, overrides: settings.overrides }),
            settings_path: settings.path,
            saving: Mutex::new(()),
            started: epoch_now(),
            store,
            items: Mutex::new(HashMap::new()),
        })
    }

    fn cfg(&self) -> Arc<Config> {
        self.live.read().unwrap_or_else(|p| p.into_inner()).cfg.clone()
    }

    fn jf(&self) -> Jellyfin {
        self.live.read().unwrap_or_else(|p| p.into_inner()).jf.clone()
    }

    /// For the navigation header: is Jellyfin answering, and what is it called.
    pub async fn status(&self) -> StatusView {
        match self.jf().public_info().await {
            Ok(i) => StatusView { connected: true, server_name: i.server_name, version: VERSION },
            Err(_) => StatusView { connected: false, server_name: None, version: VERSION },
        }
    }

    pub async fn settings(&self) -> Result<SettingsView> {
        let (cfg, overrides) = {
            let l = self.live.read().unwrap_or_else(|p| p.into_inner());
            (l.cfg.clone(), l.overrides.clone())
        };
        let env = |k: &str| std::env::var(k).ok();
        let src = |k| settings::source(k, &overrides, &env);
        let jf = self.jf();
        let (info, user) = tokio::join!(jf.public_info(), jf.current_user());
        let jellyfin = match (info, user) {
            (Ok(i), Ok(u)) => JellyfinStatus {
                ok: true,
                server_name: i.server_name,
                version: Some(i.version),
                user_name: Some(u.name),
                error: None,
            },
            (Ok(i), Err(e)) => JellyfinStatus {
                ok: false,
                server_name: i.server_name,
                version: Some(i.version),
                user_name: None,
                error: Some(format!("{e:#}")),
            },
            (Err(e), _) => JellyfinStatus { ok: false, server_name: None, version: None, user_name: None, error: Some(format!("{e:#}")) },
        };
        Ok(SettingsView {
            values: SettingsValues {
                jellyfin_url: cfg.jellyfin_url.clone(),
                api_key_set: !cfg.api_key.is_empty(),
                user_id: cfg.user_id.clone(),
                player_client: cfg.player_client.clone(),
                music_prefixes: cfg.music_prefixes.clone(),
                jellyfin_playlist_dir: cfg.jellyfin_playlist_dir.clone(),
            },
            sources: [
                ("jellyfin_url", src(settings::URL)),
                ("api_key", src(settings::KEY)),
                ("user_id", src(settings::USER)),
                ("player_client", src(settings::CLIENT)),
                ("music_prefixes", src(settings::PREFIXES)),
                ("jellyfin_playlist_dir", src(settings::JF_PLAYLIST_DIR)),
            ]
            .into(),
            fixed: FixedSettings { bind: cfg.bind.clone(), playlist_dir: cfg.playlist_dir.clone() },
            settings_file: self.settings_path.as_ref().map(|p| p.display().to_string()),
            jellyfin,
            playlists: self.playlist_count().await?,
            version: VERSION,
            started: self.started,
        })
    }

    /// Try a Jellyfin address and key without saving: the server's name and its users. An empty key means the
    /// saved one, which is only offered to the saved address (see `settings::ChangeError::KeyNeeded`).
    pub async fn probe(&self, url: &str, api_key: Option<&str>) -> Result<ProbeView, ToggleError> {
        let cfg = self.cfg();
        let url = url.trim().trim_end_matches('/');
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(ToggleError::BadSettings("the address must start with http:// or https://".into()));
        }
        let key = match api_key.map(str::trim).filter(|k| !k.is_empty()) {
            Some(k) => k.to_string(),
            None if url == cfg.jellyfin_url => cfg.api_key.clone(),
            None => return Err(ChangeError::KeyNeeded.into()),
        };
        let jf = Jellyfin::new(url, &key, "").map_err(|e| ToggleError::BadSettings(format!("{e:#}")))?;
        let info = jf.public_info().await.map_err(|e| ToggleError::BadSettings(format!("cannot reach Jellyfin: {e:#}")))?;
        let users = jf.users().await.map_err(|e| ToggleError::BadSettings(format!("Jellyfin refused the API key: {e:#}")))?;
        Ok(ProbeView { server_name: info.server_name, version: info.version, users })
    }

    /// Check the change against Jellyfin, then save it and switch over. Nothing is saved if the check fails.
    pub async fn save_settings(&self, change: &Change) -> Result<SettingsView, ToggleError> {
        let Some(path) = self.settings_path.clone() else { return Err(ToggleError::SettingsReadOnly) };
        let _one_at_a_time = self.saving.lock().await;
        let (cur, saved) = {
            let l = self.live.read().unwrap_or_else(|p| p.into_inner());
            (l.cfg.clone(), l.overrides.clone())
        };
        let env = |k: &str| std::env::var(k).ok();
        let (overrides, cfg) = settings::apply(&cur, &saved, change, &env)?;
        let bad = |what: &str, e: anyhow::Error| ToggleError::BadSettings(format!("{what}: {e:#}"));
        let jf = Jellyfin::new(&cfg.jellyfin_url, &cfg.api_key, &cfg.user_id).map_err(|e| bad("bad connection settings", e))?;
        jf.public_info().await.map_err(|e| bad("cannot reach Jellyfin", e))?;
        jf.current_user().await.map_err(|e| bad("Jellyfin refused the key or does not know that user", e))?;
        let to_save = overrides.clone();
        tokio::task::spawn_blocking(move || settings::save(&path, &to_save))
            .await
            .map_err(|e| anyhow::anyhow!("saving settings: {e}"))??;
        *self.live.write().unwrap_or_else(|p| p.into_inner()) = Live { cfg: Arc::new(cfg), jf, overrides };
        self.items.lock().await.clear(); // item details may come from another server now
        tracing::info!("settings saved and applied");
        Ok(self.settings().await?)
    }

    pub async fn playlist_count(&self) -> Result<usize> {
        Ok(self.store.view(None).await?.len())
    }

    pub async fn repairs(&self) -> Result<Repairs> {
        self.store.repairs().await
    }

    /// Apply the repairs the client chose (`None` = every safe one). The store only accepts pairs it currently
    /// offers. Jellyfin is then asked to re-read each changed playlist.
    pub async fn apply_repairs(self: &Arc<Self>, only: Option<Repair>) -> Result<Vec<String>, ToggleError> {
        let wanted = match only {
            Some(one) => vec![one],
            None => self.store.repairs().await?.safe,
        };
        let changed = self.store.apply_repairs(&wanted).await?;
        self.refresh_in_jellyfin(changed.clone());
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
        self.refresh_in_jellyfin(done);
        Ok(n)
    }

    /// Album folders that look like the same release twice, each group compared. Read-only.
    pub async fn album_duplicates(&self) -> Result<Vec<crate::albums::AlbumGroup>> {
        use crate::albums::*;
        let music = std::path::PathBuf::from(&self.cfg().playlist_dir);
        let dir = music.clone();
        let folders: Vec<String> = tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
            Ok(std::fs::read_dir(&dir)?
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .collect())
        })
        .await??;
        let mut out = Vec::new();
        for group in candidate_groups(&folders) {
            let mut albums = Vec::new();
            for folder in &group {
                let (m, f) = (music.clone(), folder.clone());
                let (files, cover) = tokio::task::spawn_blocking(move || read_folder(&m, &f)).await??;
                let known = match self.jf().album_tracks(&search_term(folder), folder).await {
                    Ok(k) => k,
                    Err(e) => {
                        tracing::warn!("Jellyfin has no details for {folder:?}: {e:#}");
                        Default::default()
                    }
                };
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
            self.refresh_in_jellyfin(vec![id.to_string()]);
        }
        Ok(removed)
    }

    pub async fn create_playlist(self: &Arc<Self>, name: &str) -> Result<(), ToggleError> {
        self.store.create(name).await?;
        self.notify_files(vec![(name.to_string(), "Created")]);
        Ok(())
    }

    pub async fn rename_playlist(self: &Arc<Self>, from: &str, to: &str) -> Result<(), ToggleError> {
        self.store.rename(from, to).await?;
        self.notify_files(vec![(from.to_string(), "Deleted"), (to.to_string(), "Created")]);
        Ok(())
    }

    pub async fn delete_playlist(self: &Arc<Self>, id: &str) -> Result<(), ToggleError> {
        self.store.delete(id).await?;
        self.notify_files(vec![(id.to_string(), "Deleted")]);
        Ok(())
    }

    pub async fn restore_playlist(self: &Arc<Self>, id: &str) -> Result<(), ToggleError> {
        self.store.restore_deleted(id).await?;
        self.notify_files(vec![(id.to_string(), "Created")]);
        Ok(())
    }

    /// Best effort, in the background: tell Jellyfin that playlist files were created or deleted.
    fn notify_files(&self, changes: Vec<(String, &'static str)>) {
        let dir = self.cfg().jellyfin_playlist_dir.trim_end_matches('/').to_string();
        let changes: Vec<(String, &'static str)> = changes.into_iter().map(|(n, k)| (format!("{dir}/{n}.m3u"), k)).collect();
        let jf = self.jf();
        tokio::spawn(async move {
            if let Err(e) = jf.library_changed(&changes).await {
                tracing::warn!("could not tell Jellyfin about the playlist file change: {e:#}");
            }
        });
    }

    /// Best effort, in the background, one playlist at a time: ask Jellyfin to re-read the changed playlists.
    fn refresh_in_jellyfin(&self, names: Vec<String>) {
        let jf = self.jf();
        tokio::spawn(async move {
            for name in names {
                if let Err(e) = jf.refresh_playlist(&name).await {
                    tracing::warn!("could not ask Jellyfin to re-read playlist {name:?}: {e:#}");
                }
            }
        });
    }

    /// Re-check which playlist entries point at missing files (the library changes without the playlists).
    pub async fn refresh_dead(&self) {
        self.store.refresh_dead().await;
    }

    /// What is playing. The activity log is the primary source because Jellyfin's session record can be stale
    /// with some clients; sessions are the fallback when the log is unavailable or has no audio history.
    async fn now_playing(&self) -> Result<Option<PlayingItem>> {
        Ok(self.now_playing_since().await?.map(|(item, _)| item))
    }

    /// `now_playing`, plus when it started (epoch seconds) if the activity log was the source.
    async fn now_playing_since(&self) -> Result<Option<(PlayingItem, Option<i64>)>> {
        match self.jf().activity(100).await {
            Ok(entries) if has_audio_history(&entries) => {
                return match current_from_activity(&entries, epoch_now(), MAX_PLAYING_AGE_SECS) {
                    Some(started) => Ok(Some((self.item(&started.item_id).await?, Some(started.at)))),
                    None => Ok(None),
                };
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("activity log unavailable, falling back to sessions: {e:#}"),
        }
        let sessions = self.jf().sessions().await?;
        Ok(pick_now_playing(&sessions, &self.cfg().player_client).map(|item| (item, None)))
    }

    /// Best effort: a failed sessions call costs only the exact position, never the page.
    async fn progress_of(&self, item: &PlayingItem, started_at: Option<i64>) -> Option<Progress> {
        let sessions = self.jf().sessions().await.unwrap_or_else(|e| {
            tracing::warn!("sessions unavailable, estimating the position: {e:#}");
            Vec::new()
        });
        progress(&item.id, item.run_time_ticks, &sessions, &self.cfg().player_client, started_at, epoch_now())
    }

    async fn item(&self, id: &str) -> Result<PlayingItem> {
        if let Some(hit) = self.items.lock().await.get(id) {
            return Ok(hit.clone());
        }
        let fetched = self.jf().item(id).await?;
        let mut cache = self.items.lock().await;
        if cache.len() > 256 {
            cache.clear();
        }
        cache.insert(id.to_string(), fetched.clone());
        Ok(fetched)
    }

    /// The playlist-file form of the track's path, if it is in the curated library.
    fn entry_of(&self, item: &PlayingItem) -> Option<String> {
        item.path.as_deref().and_then(|p| library_entry(p, &self.cfg().music_prefixes))
    }

    /// The cover of the album that holds the given track. `NotFound` when the track is unknown, outside the library,
    /// or has no cover. The id is only a lookup key for Jellyfin, never a path.
    pub async fn cover(&self, id: &str, if_none_match: Option<&str>) -> crate::covers::Outcome {
        use crate::covers::{Outcome, resolve, valid_item_id};
        if !valid_item_id(id) {
            return Outcome::NotFound;
        }
        let Ok(item) = self.item(id).await else { return Outcome::NotFound };
        match self.entry_of(&item) {
            Some(entry) => resolve(std::path::Path::new(&self.cfg().playlist_dir), &entry, if_none_match).await,
            None => Outcome::NotFound,
        }
    }

    pub async fn state(&self) -> Result<StateView> {
        let np = self.now_playing_since().await?;
        let entry = np.as_ref().and_then(|(n, _)| self.entry_of(n));
        let playlists = self.store.view(entry.as_deref()).await?;
        let progress = match &np {
            Some((n, started_at)) => self.progress_of(n, *started_at).await,
            None => None,
        };
        let now_playing = np.map(|(n, _)| NowPlayingView {
            addable: entry.is_some(),
            progress,
            id: n.id,
            name: n.name,
            artist: n.album_artist,
            album: n.album,
        });
        Ok(StateView { now_playing, playlists })
    }

    /// Add the playing track to the playlist file, or remove it if it is already there.
    pub async fn toggle(self: &Arc<Self>, playlist_id: &str) -> Result<StateView, ToggleError> {
        let track = self.now_playing().await?.ok_or(ToggleError::NothingPlaying)?;
        let entry = self.entry_of(&track).ok_or(ToggleError::NotInLibrary)?;
        self.store.toggle(playlist_id, &entry).await?;
        // Best effort: ask Jellyfin to re-read that playlist so Finamp and the others see the change.
        let jf = self.jf();
        let name = playlist_id.to_string();
        tokio::spawn(async move {
            if let Err(e) = jf.refresh_playlist(&name).await {
                tracing::warn!("could not ask Jellyfin to re-read playlist {name:?}: {e:#}");
            }
        });
        Ok(self.state().await?)
    }
}
