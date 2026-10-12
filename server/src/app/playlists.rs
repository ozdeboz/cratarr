//! Playlists (decision 0011): the Music folder's playlist files, edits and repairs, Play, and telling the servers
//! about changes.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;

use super::ToggleError;
use super::library::{Library, label_of};
use crate::playlists::{DupGroup, PlaylistView, Repair, Repairs};
use crate::source::{self, PlaylistChange, PlaylistEvent};

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

pub struct PlaylistService {
    lib: Arc<Library>,
}

impl PlaylistService {
    pub(super) fn new(lib: Arc<Library>) -> Self {
        Self { lib }
    }

    /// Every playlist, marked where it holds `entry` (the playing track's line).
    pub(super) async fn view(&self, entry: Option<&str>) -> Result<Vec<PlaylistView>> {
        match self.lib.store_opt() {
            Some(store) => store.view(entry).await,
            None => Ok(Vec::new()),
        }
    }

    /// Add `entry` to the playlist, or remove it if it is there.
    pub(super) async fn toggle_entry(&self, playlist_id: &str, entry: &str) -> Result<(), ToggleError> {
        self.lib.store()?.toggle(playlist_id, entry).await?;
        self.tell_sources(vec![(playlist_id.to_string(), PlaylistChange::Edited)]);
        Ok(())
    }

    /// Play a playlist on the server's player: its tracks in file order (or shuffled), looked up in the catalogue,
    /// at most PLAY_MAX of them. `want` = the server to use (default: the first connected one).
    pub async fn play(&self, id: &str, mode: PlayMode, want: Option<&str>) -> Result<PlayResult, ToggleError> {
        let entries = self.lib.store()?.entries(id).await?;
        let sources = self.lib.connected();
        let (active, source) = want
            .and_then(|w| sources.iter().find(|(a, _)| a.cfg.id == w))
            .or_else(|| sources.first())
            .cloned()
            .ok_or_else(|| ToggleError::CannotPlay("no server is connected".into()))?;
        let (c, sid) = (self.lib.catalogue(), active.cfg.id.clone());
        let found = tokio::task::spawn_blocking(move || c.track_ids(&sid, 0, &entries))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))??;
        let unknown = found.iter().filter(|f| f.is_none()).count();
        let mut ids: Vec<String> = found.into_iter().flatten().collect();
        let total = ids.len();
        if total == 0 {
            // Usually the catalogue is still matching files to the server (a first fill takes minutes): say so.
            let c = self.lib.catalogue();
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

    /// Best effort, in the background: every connected source hears about changed playlist files, each with the
    /// file's path as it sees it.
    fn tell_sources(&self, changes: Vec<(String, PlaylistChange)>) {
        let Some(music) = self.lib.music() else { return };
        for (a, s) in self.lib.connected() {
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

    /// 0 when there is no Music folder.
    pub async fn count(&self) -> Result<usize> {
        Ok(self.view(None).await?.len())
    }

    pub async fn repairs(&self) -> Result<Repairs> {
        self.lib.store()?.repairs().await
    }

    /// Apply the repairs the client chose (`None` = every safe one). The store only accepts pairs it currently
    /// offers. The sources are then told about each changed playlist.
    pub async fn apply_repairs(&self, only: Option<Repair>) -> Result<Vec<String>, ToggleError> {
        let wanted = match only {
            Some(one) => vec![one],
            None => self.lib.store()?.repairs().await?.safe,
        };
        let changed = self.lib.store()?.apply_repairs(&wanted).await?;
        self.tell_sources(changed.iter().map(|n| (n.clone(), PlaylistChange::Edited)).collect());
        Ok(changed)
    }

    /// Put each playlist back as it was before its last change.
    pub async fn undo(&self, playlists: Vec<String>) -> Result<usize, ToggleError> {
        let mut done = Vec::new();
        for id in playlists {
            self.lib.store()?.undo(&id).await?;
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
        let Some(music) = self.lib.music().map(PathBuf::from) else { return Ok(Vec::new()) };
        let store = self.lib.store()?;
        let dir = music.clone();
        let folders: Vec<String> = tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
            Ok(std::fs::read_dir(&dir)?
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                .collect())
        })
        .await??;
        let sources = self.lib.connected();
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
        self.lib.store()?.duplicates().await
    }

    /// Remove repeated entries from one playlist (the first copy of each stays).
    pub async fn dedupe(&self, id: &str) -> Result<usize, ToggleError> {
        let removed = self.lib.store()?.dedupe(id).await?;
        if removed > 0 {
            self.tell_sources(vec![(id.to_string(), PlaylistChange::Edited)]);
        }
        Ok(removed)
    }

    pub async fn create(&self, name: &str) -> Result<(), ToggleError> {
        self.lib.store()?.create(name).await?;
        self.tell_sources(vec![(name.to_string(), PlaylistChange::Created)]);
        Ok(())
    }

    pub async fn rename(&self, from: &str, to: &str) -> Result<(), ToggleError> {
        self.lib.store()?.rename(from, to).await?;
        self.tell_sources(vec![(from.to_string(), PlaylistChange::Deleted), (to.to_string(), PlaylistChange::Created)]);
        Ok(())
    }

    pub async fn delete(&self, id: &str) -> Result<(), ToggleError> {
        self.lib.store()?.delete(id).await?;
        self.tell_sources(vec![(id.to_string(), PlaylistChange::Deleted)]);
        Ok(())
    }

    pub async fn restore(&self, id: &str) -> Result<(), ToggleError> {
        self.lib.store()?.restore_deleted(id).await?;
        self.tell_sources(vec![(id.to_string(), PlaylistChange::Created)]);
        Ok(())
    }

    /// Re-check which playlist entries point at missing files (the library changes without the playlists).
    pub async fn refresh_dead(&self) {
        if let Some(store) = self.lib.store_opt() {
            store.refresh_dead().await;
        }
    }
}
