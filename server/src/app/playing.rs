//! What plays (decision 0011): asks the servers, picks one, and the actions on what plays (the controls, adding it
//! to a playlist, its cover).

use std::sync::Arc;

use anyhow::Result;
use serde::Serialize;

use super::ToggleError;
use super::library::{Active, Library, SourceTab, label_of};
use super::playlists::PlaylistService;
use crate::nowplaying::Progress;
use crate::playlists::PlaylistView;
use crate::source::{self, Playing};

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
    /// Whether the player's account has it as a favourite; `None` when not known.
    pub favorite: Option<bool>,
    /// What plays next, in order, as far as the player reports (the page moves on at once when Next is pressed).
    pub upcoming: Vec<UpcomingView>,
}

/// A track the player plays after the current one.
#[derive(Debug, Serialize, PartialEq)]
pub struct UpcomingView {
    pub id: String,
    pub name: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_id: Option<String>,
    pub addable: bool,
    pub duration_secs: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct StateView {
    pub now_playing: Option<NowPlayingView>,
    pub playlists: Vec<PlaylistView>,
    pub sources: Vec<SourceTab>,
}

pub struct PlayingService {
    lib: Arc<Library>,
    playlists: Arc<PlaylistService>,
}

impl PlayingService {
    pub(super) fn new(lib: Arc<Library>, playlists: Arc<PlaylistService>) -> Self {
        Self { lib, playlists }
    }

    /// Ask every connected source what it plays, all at once, and pick: the requested source, or (`None` / unknown)
    /// the one whose track started most recently.
    async fn playing(&self, want: Option<&str>) -> Result<(Option<(Arc<Active>, Playing)>, Vec<SourceTab>)> {
        let sources = self.lib.connected();
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
            Some((a, _)) => self.lib.connected().into_iter().find(|(c, _)| c.cfg.id == a.cfg.id).map(|(_, s)| s),
            None => None,
        }
        .ok_or_else(|| ToggleError::CannotPlay("nothing is playing".into()))?;
        source.transport(command).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
    }

    /// Mark the playing track a favourite (`on`) or not, for the account of its player. Safe to repeat.
    pub async fn set_favorite(&self, want: Option<&str>, on: bool) -> Result<StateView, ToggleError> {
        let (np, _) = self.playing(want).await?;
        let (active, playing) = np.ok_or(ToggleError::NothingPlaying)?;
        let source = self.lib.connected().into_iter().find(|(a, _)| a.cfg.id == active.cfg.id).map(|(_, s)| s);
        let source = source.ok_or(ToggleError::NothingPlaying)?;
        source.set_favorite(&playing.track.id, on).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))?;
        Ok(self.state(want).await?)
    }

    pub async fn state(&self, want: Option<&str>) -> Result<StateView> {
        let (np, sources) = self.playing(want).await?;
        let music = self.lib.music();
        let entry = np.as_ref().and_then(|(a, p)| a.entry(music.as_deref(), &p.track));
        let playlists = self.playlists.view(entry.as_deref()).await?;
        let now_playing = np.map(|(a, p)| NowPlayingView {
            upcoming: p
                .upcoming
                .into_iter()
                .map(|u| UpcomingView {
                    addable: a.entry(music.as_deref(), &u.track).is_some(),
                    id: u.track.id,
                    name: u.track.title,
                    artist: u.track.artist,
                    album: u.track.album,
                    album_id: u.track.album_id,
                    duration_secs: u.duration_secs,
                })
                .collect(),
            source: a.cfg.id.clone(),
            addable: entry.is_some(),
            progress: p.progress,
            id: p.track.id,
            name: p.track.title,
            artist: p.track.artist,
            album: p.track.album,
            album_id: p.track.album_id,
            player: p.player,
            favorite: p.favorite,
        });
        Ok(StateView { now_playing, playlists, sources })
    }

    /// Put the playing track in the playlist (`member: Some(true)`), take it out (`Some(false)`), or flip it (`None`).
    /// With `Some`, a repeated or retried request changes nothing more.
    pub async fn toggle(&self, playlist_id: &str, want: Option<&str>, member: Option<bool>) -> Result<StateView, ToggleError> {
        let (np, _) = self.playing(want).await?;
        let (active, playing) = np.ok_or(ToggleError::NothingPlaying)?;
        self.lib.store()?;
        let entry = active.entry(self.lib.music().as_deref(), &playing.track).ok_or(ToggleError::NotInLibrary)?;
        self.playlists.set_entry(playlist_id, &entry, member).await?;
        Ok(self.state(want).await?)
    }

    /// The cover of the album that holds the given track. `NotFound` when the source or track is unknown, the track
    /// is not in a library in use, or there is no cover. The ids are only lookup keys, never paths.
    pub async fn cover(&self, source_id: &str, id: &str, if_none_match: Option<&str>) -> crate::covers::Outcome {
        use crate::covers::{Outcome, resolve, valid_item_id};
        if !valid_item_id(id) {
            return Outcome::NotFound;
        }
        let Some((a, s)) = self.lib.connected().into_iter().find(|(a, _)| a.cfg.id == source_id) else { return Outcome::NotFound };
        let Ok(track) = s.track(id).await else { return Outcome::NotFound };
        let Some(music) = self.lib.music() else { return Outcome::NotFound };
        match a.entry(Some(&music), &track) {
            Some(entry) => resolve(std::path::Path::new(&music), &entry, if_none_match).await,
            None => Outcome::NotFound,
        }
    }
}
