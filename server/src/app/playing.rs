//! What plays (decision 0011): asks the servers, picks one, and the actions on what plays (the controls, adding it
//! to a playlist, its cover).

use std::sync::{Arc, Mutex};

use anyhow::Result;
use serde::Serialize;

use super::ToggleError;
use super::library::{Active, Library, SourceTab, label_of};
use super::playlists::PlaylistService;
use crate::nowplaying::Progress;
use crate::playlists::PlaylistView;
use crate::source::{self, Playing, TrackInfo};

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
    /// What the server knows about the file and its plays (the info chips, decision 0019).
    pub info: Option<TrackInfo>,
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

/// A track the page plays itself (decision 0022) as the player shows it: its playlists, and whether it can be added
/// to them or is a favourite. The same as for what the server's player plays, but for the track asked about.
#[derive(Debug, Serialize)]
pub struct TrackStateView {
    pub addable: bool,
    /// `None` when the source cannot tell (no account to ask for).
    pub favorite: Option<bool>,
    pub playlists: Vec<PlaylistView>,
    pub info: Option<TrackInfo>,
}

/// A playlist made from the queue: how many tracks it got, and how many were left out (not in the library).
#[derive(Debug, Serialize)]
pub struct FromQueue {
    pub playlist: String,
    pub added: usize,
    pub left_out: usize,
}

/// An album whose cover the page can show (`/api/incoming/cover/{source}/{album_id}`).
#[derive(Debug, Serialize, PartialEq)]
pub struct AlbumRef {
    pub source: String,
    pub album_id: String,
}

pub struct PlayingService {
    lib: Arc<Library>,
    playlists: Arc<PlaylistService>,
    /// The info chips of the last track asked about: the page asks for the state every few seconds, the file does not
    /// change meanwhile (the play count is read again for the next track).
    info: Mutex<Option<(String, Option<TrackInfo>)>>,
}

impl PlayingService {
    pub(super) fn new(lib: Arc<Library>, playlists: Arc<PlaylistService>) -> Self {
        Self { lib, playlists, info: Mutex::new(None) }
    }

    /// A track's info chips, from the cache when it is the last one asked about. `None` when the source cannot tell.
    async fn info(&self, s: &Arc<dyn source::Source>, source_id: &str, id: &str) -> Option<TrackInfo> {
        let key = format!("{source_id}/{id}");
        if let Some((k, v)) = self.info.lock().unwrap_or_else(|p| p.into_inner()).as_ref()
            && *k == key
        {
            return v.clone();
        }
        let got = s.track_info(id).await.unwrap_or_else(|e| {
            tracing::warn!("info for {key}: {e:#}");
            None
        });
        *self.info.lock().unwrap_or_else(|p| p.into_inner()) = Some((key, got.clone()));
        got
    }

    /// The connected source with that id.
    fn source(&self, source_id: &str) -> Result<(Arc<Active>, Arc<dyn source::Source>), ToggleError> {
        self.lib
            .connected()
            .into_iter()
            .find(|(a, _)| a.cfg.id == source_id)
            .ok_or_else(|| ToggleError::Refused("That server is not connected.".into()))
    }

    /// The source to send a queue command to: the asked one, else the one whose player plays.
    async fn queue_source(&self, want: Option<&str>) -> Result<(Arc<Active>, Arc<dyn source::Source>, Playing), ToggleError> {
        let (np, _) = self.playing(want).await?;
        let (active, playing) = np.ok_or(ToggleError::NothingPlaying)?;
        let (_, s) = self.source(&active.cfg.id)?;
        Ok((active, s, playing))
    }

    /// Play a track next, or at the end of the queue, on the player that plays (the track menu, decision 0019).
    pub async fn queue_add(&self, want: Option<&str>, id: &str, how: source::PlayHow) -> Result<String, ToggleError> {
        let (_, s, _) = self.queue_source(want).await?;
        s.play_tracks(&[id.to_string()], how).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
    }

    /// The player plays from this track of its queue on (the queue view's tap, decision 0019).
    pub async fn queue_jump(&self, want: Option<&str>, id: &str) -> Result<String, ToggleError> {
        let (_, s, _) = self.queue_source(want).await?;
        s.jump_to(id).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
    }

    /// A new playlist of these tracks (`ids` on `want`, the local player's queue), or else of what the server's player
    /// plays and what comes next; tracks outside the library are left out (decision 0019).
    pub async fn playlist_from_queue(&self, want: Option<&str>, name: &str, ids: Option<Vec<String>>) -> Result<FromQueue, ToggleError> {
        let (active, s, ids) = match ids {
            Some(ids) => {
                let (a, s) = self.source(want.ok_or_else(|| ToggleError::Refused("Which server are the tracks on?".into()))?)?;
                (a, s, ids)
            }
            None => {
                let (a, s, p) = self.queue_source(want).await?;
                let ids = std::iter::once(p.track.id).chain(p.upcoming.into_iter().map(|u| u.track.id)).collect();
                (a, s, ids)
            }
        };
        self.lib.store()?;
        let music = self.lib.music();
        let mut entries = Vec::new();
        let mut left_out = 0;
        for id in &ids {
            let entry = match s.track(id).await {
                Ok(t) => active.entry(music.as_deref(), &t),
                Err(_) => None,
            };
            match entry {
                // A track twice in the queue is once in the playlist.
                Some(e) if !entries.contains(&e) => entries.push(e),
                Some(_) => {}
                None => left_out += 1,
            }
        }
        if entries.is_empty() {
            return Err(ToggleError::Refused("None of those tracks are in your Music.".into()));
        }
        self.playlists.create_with(name, &entries).await?;
        Ok(FromQueue { playlist: name.to_string(), added: entries.len(), left_out })
    }

    /// Up to four albums of a playlist, in its order, for the mosaic on its menu (decision 0021).
    pub async fn playlist_covers(&self, playlist_id: &str, want: Option<&str>) -> Result<Vec<AlbumRef>, ToggleError> {
        let (source_id, ids, _) = self.playlists.track_ids(playlist_id, want).await?;
        let (_, s) = self.source(&source_id)?;
        let mut albums: Vec<AlbumRef> = Vec::new();
        // A few tracks per album are usual: looking at the first 40 finds four albums in most playlists.
        for id in ids.iter().take(40) {
            let Ok(t) = s.track(id).await else { continue };
            let Some(album_id) = t.album_id else { continue };
            if !albums.iter().any(|a| a.album_id == album_id) {
                albums.push(AlbumRef { source: source_id.clone(), album_id });
                if albums.len() == 4 {
                    break;
                }
            }
        }
        Ok(albums)
    }

    /// Count a play of a track the page played itself (decision 0022).
    pub async fn count_play(&self, source_id: &str, id: &str) -> Result<(), ToggleError> {
        let (_, s, _) = self.find(source_id, id).await?;
        s.count_play(id).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))?;
        // The play count changed: the next state asks again.
        *self.info.lock().unwrap_or_else(|p| p.into_inner()) = None;
        Ok(())
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
        let info = match &np {
            Some((a, p)) => match self.source(&a.cfg.id) {
                Ok((_, s)) => self.info(&s, &a.cfg.id, &p.track.id).await,
                Err(_) => None,
            },
            None => None,
        };
        let now_playing = np.map(|(a, p)| NowPlayingView {
            info,
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

    /// The connected source with that id and its track with that id.
    async fn find(&self, source_id: &str, id: &str) -> Result<(Arc<Active>, Arc<dyn source::Source>, source::Track), ToggleError> {
        let gone = || ToggleError::Refused("That track is not on a connected server.".into());
        if !crate::covers::valid_item_id(id) {
            return Err(gone());
        }
        let (a, s) = self.lib.connected().into_iter().find(|(a, _)| a.cfg.id == source_id).ok_or_else(gone)?;
        let track = s.track(id).await.map_err(|_| gone())?;
        Ok((a, s, track))
    }

    /// A given track's playlists, favourite and info chips (the local player's, decision 0022).
    pub async fn track_state(&self, source_id: &str, id: &str) -> Result<TrackStateView, ToggleError> {
        let (a, s, track) = self.find(source_id, id).await?;
        let entry = a.entry(self.lib.music().as_deref(), &track);
        let (favorite, info) = tokio::join!(s.favorite(id), self.info(&s, source_id, id));
        let playlists = self.playlists.view(entry.as_deref()).await?;
        Ok(TrackStateView { addable: entry.is_some(), favorite: favorite.unwrap_or(None), playlists, info })
    }

    /// Puts a given track in the playlist or takes it out, as `toggle` does for what the server's player plays.
    pub async fn track_toggle(&self, source_id: &str, id: &str, playlist_id: &str, member: Option<bool>) -> Result<TrackStateView, ToggleError> {
        let (a, _, track) = self.find(source_id, id).await?;
        self.lib.store()?;
        let entry = a.entry(self.lib.music().as_deref(), &track).ok_or(ToggleError::NotInLibrary)?;
        self.playlists.set_entry(playlist_id, &entry, member).await?;
        self.track_state(source_id, id).await
    }

    /// Which of these tracks are favourites, in one go (the hearts on a list's rows, decision 0023).
    pub async fn favorites(&self, source_id: &str, ids: &[String]) -> Result<std::collections::BTreeMap<String, bool>, ToggleError> {
        let (_, s) = self.source(source_id)?;
        s.favorites(ids).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
    }

    /// Marks a given track a favourite, or not.
    pub async fn track_favorite(&self, source_id: &str, id: &str, on: bool) -> Result<TrackStateView, ToggleError> {
        let (_, s, _) = self.find(source_id, id).await?;
        s.set_favorite(id, on).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))?;
        self.track_state(source_id, id).await
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
