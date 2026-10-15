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

#[derive(Debug, Clone, Serialize, PartialEq)]
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
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct UpcomingView {
    pub id: String,
    pub name: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_id: Option<String>,
    pub addable: bool,
    pub duration_secs: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StateView {
    pub now_playing: Option<NowPlayingView>,
    pub playlists: Vec<PlaylistView>,
    pub sources: Vec<SourceTab>,
}

/// The most genres the panel shows on top of an album or a crate.
pub const GENRES_SHOWN: usize = 5;

/// An album as its view in the panel shows it (decision 0023): what its tracks say of it, and its tracks in play order
/// as a playlist's rows. A row's `entry` is the server's path of the file.
#[derive(Debug, Serialize)]
pub struct AlbumView {
    pub source: String,
    /// The album's id: another than asked for when the album moved (Keep, Replace) and this is where it is now.
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub year: Option<u32>,
    /// Its tracks' genres, most shared first.
    pub genres: Vec<String>,
    /// "FLAC", "MP3"…: `None` when mixed, unknown, or the source cannot say (the header's detail line).
    pub format: Option<String>,
    /// Its weakest track's bitrate.
    pub bitrate_kbps: Option<u32>,
    pub rows: Vec<super::playlists::PlaylistRow>,
}

/// A track the page plays itself (decision 0022) as the player shows it: its playlists, and whether it can be added
/// to them or is a favourite. The same as for what the server's player plays, but for the track asked about.
#[derive(Debug, Serialize)]
pub struct TrackStateView {
    pub addable: bool,
    /// The server's id of the track's album: View album.
    pub album_id: Option<String>,
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
    /// Its undo token (decision 0026).
    pub undo: Option<String>,
}

/// An album whose cover the page can show (`/api/albums/{source}/{album_id}/cover`).
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
    /// The last state worked out: when, for which source followed, and it. Whoever asks within `STATE_REUSE` gets it,
    /// and one asks at a time: the pages (and the live channel) asking often, or a page caught in a loop, cost the media
    /// server one question at a time at most (2026-10-14: one tab's loop of 200 a second took it down).
    state_kept: tokio::sync::Mutex<Option<(std::time::Instant, Option<String>, StateView)>>,
}

/// How long a state worked out is answered again.
const STATE_REUSE: std::time::Duration = std::time::Duration::from_secs(1);

impl PlayingService {
    pub(super) fn new(lib: Arc<Library>, playlists: Arc<PlaylistService>) -> Self {
        Self { lib, playlists, info: Mutex::new(None), state_kept: tokio::sync::Mutex::new(None) }
    }

    /// What plays and which playlists hold it: as worked out within the last second, else afresh.
    pub async fn state(&self, want: Option<&str>) -> Result<StateView> {
        let mut kept = self.state_kept.lock().await;
        if let Some((at, w, view)) = kept.as_ref()
            && w.as_deref() == want
            && at.elapsed() < STATE_REUSE
        {
            return Ok(view.clone());
        }
        let view = self.work_out_state(want).await?;
        *kept = Some((std::time::Instant::now(), want.map(str::to_string), view.clone()));
        Ok(view)
    }

    /// After a change (a favourite, a playlist's membership): worked out afresh, and kept for whoever asks next.
    async fn fresh_state(&self, want: Option<&str>) -> Result<StateView> {
        let mut kept = self.state_kept.lock().await;
        let view = self.work_out_state(want).await?;
        *kept = Some((std::time::Instant::now(), want.map(str::to_string), view.clone()));
        Ok(view)
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

    /// Play tracks next, or at the end of the queue, in order, on the player that plays (the track menu, decision 0019; a
    /// crate's tracks, decision 0024).
    pub async fn queue_add(&self, want: Option<&str>, ids: &[String], how: source::PlayHow) -> Result<String, ToggleError> {
        let (_, s, _) = self.queue_source(want).await?;
        s.play_tracks(ids, how).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
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
        let folders = self.lib.folders();
        let mut entries = Vec::new();
        let mut left_out = 0;
        for id in &ids {
            let entry = match s.track(id).await {
                Ok(t) => active.line(&folders, &t),
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
        let undo = self.playlists.create_with(name, &entries).await?;
        Ok(FromQueue { playlist: name.to_string(), added: entries.len(), left_out, undo })
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
        self.count_play_at(source_id, id, None).await
    }

    /// Count a play, played at `at` (epoch seconds) when a player tells it late (decision 0033); now without.
    pub async fn count_play_at(&self, source_id: &str, id: &str, at: Option<i64>) -> Result<(), ToggleError> {
        let (s, id) = match self.find(source_id, id).await {
            Ok((_, s, _)) => (s, id.to_string()),
            // A track of a server no longer on (Jellyfin, once Cratarr is its own library, decision 0032), still in a
            // queue: its play counts in Cratarr's own library, by the file the catalogue links it to.
            Err(gone) => {
                let (cat, src, item) = (self.lib.catalogue(), source_id.to_string(), id.to_string());
                let own = tokio::task::spawn_blocking(move || cat.track_of_source(&src, &item)).await.ok().flatten().ok_or(gone)?;
                let own = crate::serve::ids::song(own);
                let (_, s, _) = self.find(crate::source::cratarr::KIND.id, &own).await?;
                (s, own)
            }
        };
        let counted = match at {
            Some(at) => s.count_play_at(&id, at).await,
            None => s.count_play(&id).await,
        };
        counted.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))?;
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
        // The crates' kept favourites take it at once (decision 0024).
        crate::crates::favourite_changed(&active.cfg.id, &playing.track.id, on);
        Ok(self.fresh_state(want).await?)
    }

    async fn work_out_state(&self, want: Option<&str>) -> Result<StateView> {
        let (np, sources) = self.playing(want).await?;
        let folders = self.lib.folders();
        let entry = np.as_ref().and_then(|(a, p)| a.line(&folders, &p.track));
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
                    addable: a.line(&folders, &u.track).is_some(),
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
        let entry = active.line(&self.lib.folders(), &playing.track).ok_or(ToggleError::NotInLibrary)?;
        self.playlists.set_entry(playlist_id, &entry, member).await?;
        Ok(self.fresh_state(want).await?)
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
        let entry = a.line(&self.lib.folders(), &track);
        let (favorite, info) = tokio::join!(s.favorite(id), self.info(&s, source_id, id));
        let playlists = self.playlists.view(entry.as_deref()).await?;
        Ok(TrackStateView { addable: entry.is_some(), album_id: track.album_id.clone(), favorite: favorite.unwrap_or(None), playlists, info })
    }

    /// Puts a given track in the playlist or takes it out, as `toggle` does for what the server's player plays.
    pub async fn track_toggle(&self, source_id: &str, id: &str, playlist_id: &str, member: Option<bool>) -> Result<TrackStateView, ToggleError> {
        let (a, _, track) = self.find(source_id, id).await?;
        self.lib.store()?;
        let entry = a.line(&self.lib.folders(), &track).ok_or(ToggleError::NotInLibrary)?;
        self.playlists.set_entry(playlist_id, &entry, member).await?;
        self.track_state(source_id, id).await
    }

    /// Which of these tracks are favourites, in one go (the hearts on a list's rows, decision 0023).
    pub async fn favorites(&self, source_id: &str, ids: &[String]) -> Result<std::collections::BTreeMap<String, bool>, ToggleError> {
        let (_, s) = self.source(source_id)?;
        s.favorites(ids).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
    }

    /// Makes an album a favourite or not (the album panel's heart). Its tracks keep their own hearts.
    pub async fn album_favorite(&self, source_id: &str, album_id: &str, on: bool) -> Result<bool, ToggleError> {
        if !crate::covers::valid_item_id(album_id) {
            return Err(ToggleError::Refused("That is not an album.".into()));
        }
        let (_, s) = self.source(source_id)?;
        s.set_favorite(album_id, on).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
    }

    /// An album of a connected server, for its view in the panel.
    pub async fn album_view(&self, source_id: &str, album_id: &str) -> Result<AlbumView, ToggleError> {
        if !crate::covers::valid_item_id(album_id) {
            return Err(ToggleError::Refused("That is not an album.".into()));
        }
        let (_, s) = self.source(source_id)?;
        // The server's own words (a request and its status) go to the log, not the page.
        let tracks = s.tracks_of_album(album_id).await.map_err(|e| {
            tracing::warn!("{source_id}: album {album_id}: {e:#}");
            ToggleError::Refused("The server could not read this album.".into())
        })?;
        let first = tracks.first();
        let (title, artist, year) =
            (first.and_then(|t| t.album.clone()).unwrap_or_default(), first.and_then(|t| t.album_artist.clone()), first.and_then(|t| t.year));
        let (c, sid, ids) = (self.lib.catalogue(), source_id.to_string(), tracks.iter().map(|t| t.id.clone()).collect::<Vec<_>>());
        let per_track = tokio::task::spawn_blocking(move || c.genres_of_ids(&sid, &ids)).await.unwrap_or_default();
        let genres = crate::crates::top_genres(per_track.iter().map(|g| g.as_slice()), GENRES_SHOWN);
        // What the source knows of the files (Cratarr's own library does; a source that cannot say leaves them out).
        let details = s.album_summary(album_id).await.ok().flatten();
        let (format, bitrate_kbps) = details.map_or((None, None), |d| (d.format, d.bitrate_kbps));
        let rows = tracks
            .into_iter()
            .map(|t| {
                let entry = t.path.unwrap_or_else(|| t.id.clone());
                super::playlists::PlaylistRow {
                    file: entry.rsplit('/').next().unwrap_or(&entry).to_string(),
                    entry,
                    id: Some(t.id),
                    name: Some(t.title),
                    artist: t.artist,
                    album: t.album,
                    duration_secs: t.duration_secs,
                    missing: false,
                }
            })
            .collect();
        Ok(AlbumView { source: source_id.to_string(), id: album_id.to_string(), title, artist, year, genres, format, bitrate_kbps, rows })
    }

    /// Plays an album of a connected server on that server's player. Returns the player's label.
    pub async fn play_album(&self, source_id: &str, album_id: &str) -> Result<String, ToggleError> {
        if !crate::covers::valid_item_id(album_id) {
            return Err(ToggleError::Refused("That is not an album.".into()));
        }
        let (_, s) = self.source(source_id)?;
        s.play_album(album_id).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
    }

    /// An album's image from a connected server (the album lists, the panel, the now-playing header for music
    /// outside the library). Only images come back, and the id is only ever passed on to that server.
    pub async fn album_cover(&self, source_id: &str, album_id: &str) -> Option<(String, Vec<u8>)> {
        if !crate::covers::valid_item_id(album_id) {
            return None;
        }
        let (_, s) = self.source(source_id).ok()?;
        match s.album_cover(album_id).await {
            Ok(found) => found,
            Err(e) => {
                tracing::warn!("cover of {album_id} on {source_id}: {e:#}");
                None
            }
        }
    }

    /// Marks a given track a favourite, or not.
    pub async fn track_favorite(&self, source_id: &str, id: &str, on: bool) -> Result<TrackStateView, ToggleError> {
        let (_, s, _) = self.find(source_id, id).await?;
        s.set_favorite(id, on).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))?;
        crate::crates::favourite_changed(source_id, id, on);
        self.track_state(source_id, id).await
    }

    /// The cover of the album that holds the given track. `NotFound` when the source or track is unknown, the track
    /// is not in a library in use, or there is no cover. The ids are only lookup keys, never paths.
    pub async fn cover(&self, source_id: &str, id: &str, if_none_match: Option<&str>) -> crate::covers::Outcome {
        use crate::covers::{Outcome, resolve, valid_item_id};
        if !valid_item_id(id) {
            return Outcome::NotFound;
        }
        // Where the catalogue has the track: in any of its folders, and for Cratarr's own ids without the source
        // connected (decision 0032).
        let (cat, source, item) = (self.lib.catalogue(), source_id.to_string(), id.to_string());
        let place = tokio::task::spawn_blocking(move || {
            let (folder, rel) = cat.file_of(cat.track_of_source(&source, &item)?)?;
            Some((cat.folders().get(usize::try_from(folder).ok()?)?.clone(), rel))
        })
        .await
        .ok()
        .flatten();
        if let Some((root, rel)) = place {
            return resolve(&root, &rel, if_none_match).await;
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
