//! Application logic: works out what is playing and which playlist files contain it, and decides what a tap does.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;
use tokio::sync::Mutex;

use crate::config::Config;
use crate::jellyfin::{Jellyfin, PlayingItem, pick_now_playing};
use crate::m3u::library_entry;
use crate::nowplaying::{current_from_activity, has_audio_history};
use crate::playlists::{DupGroup, PlaylistView, Repair, Repairs, Store, StoreError};

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct NowPlayingView {
    pub id: String,
    pub name: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// False when the track is outside the curated library (an incoming folder, say): it cannot be added.
    pub addable: bool,
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
    #[error("{0:#}")]
    Upstream(#[from] anyhow::Error),
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

/// A track that started longer ago than this without a stop entry is assumed to be a stale log line.
const MAX_PLAYING_AGE_SECS: i64 = 3 * 3600;

fn epoch_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub struct App {
    cfg: Config,
    jf: Jellyfin,
    store: Store,
    /// Details of recently seen tracks, so polling the playing track costs one cheap request.
    items: Mutex<HashMap<String, PlayingItem>>,
}

impl App {
    pub fn new(cfg: Config, jf: Jellyfin) -> Arc<Self> {
        let store = Store::new(&cfg.playlist_dir);
        Arc::new(Self { cfg, jf, store, items: Mutex::new(HashMap::new()) })
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
        let dir = self.cfg.jellyfin_playlist_dir.trim_end_matches('/').to_string();
        let changes: Vec<(String, &'static str)> = changes.into_iter().map(|(n, k)| (format!("{dir}/{n}.m3u"), k)).collect();
        let jf = self.jf.clone();
        tokio::spawn(async move {
            if let Err(e) = jf.library_changed(&changes).await {
                tracing::warn!("could not tell Jellyfin about the playlist file change: {e:#}");
            }
        });
    }

    /// Best effort, in the background, one playlist at a time: ask Jellyfin to re-read the changed playlists.
    fn refresh_in_jellyfin(&self, names: Vec<String>) {
        let jf = self.jf.clone();
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
        match self.jf.activity(100).await {
            Ok(entries) if has_audio_history(&entries) => {
                return match current_from_activity(&entries, epoch_now(), MAX_PLAYING_AGE_SECS) {
                    Some(id) => Ok(Some(self.item(&id).await?)),
                    None => Ok(None),
                };
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("activity log unavailable, falling back to sessions: {e:#}"),
        }
        let sessions = self.jf.sessions().await?;
        Ok(pick_now_playing(&sessions, &self.cfg.player_client))
    }

    async fn item(&self, id: &str) -> Result<PlayingItem> {
        if let Some(hit) = self.items.lock().await.get(id) {
            return Ok(hit.clone());
        }
        let fetched = self.jf.item(id).await?;
        let mut cache = self.items.lock().await;
        if cache.len() > 256 {
            cache.clear();
        }
        cache.insert(id.to_string(), fetched.clone());
        Ok(fetched)
    }

    /// The playlist-file form of the track's path, if it is in the curated library.
    fn entry_of(&self, item: &PlayingItem) -> Option<String> {
        item.path.as_deref().and_then(|p| library_entry(p, &self.cfg.music_prefixes))
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
            Some(entry) => resolve(std::path::Path::new(&self.cfg.playlist_dir), &entry, if_none_match).await,
            None => Outcome::NotFound,
        }
    }

    pub async fn state(&self) -> Result<StateView> {
        let np = self.now_playing().await?;
        let entry = np.as_ref().and_then(|n| self.entry_of(n));
        let playlists = self.store.view(entry.as_deref()).await?;
        let now_playing = np.map(|n| NowPlayingView {
            addable: entry.is_some(),
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
        let jf = self.jf.clone();
        let name = playlist_id.to_string();
        tokio::spawn(async move {
            if let Err(e) = jf.refresh_playlist(&name).await {
                tracing::warn!("could not ask Jellyfin to re-read playlist {name:?}: {e:#}");
            }
        });
        Ok(self.state().await?)
    }
}
