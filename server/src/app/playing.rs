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

/// One track of several albums' (`Playing::tracks_of_albums`), as the web's album tracks are.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AlbumsTrack {
    pub source: String,
    pub id: String,
    pub name: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_id: String,
    pub duration_secs: Option<f64>,
}

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
    /// Who asked for it, a queue request accepted (0.10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asked_by: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StateView {
    pub now_playing: Option<NowPlayingView>,
    pub playlists: Vec<PlaylistView>,
    pub sources: Vec<SourceTab>,
}

/// The most genres the panel shows on top of an album or a crate.
pub const GENRES_SHOWN: usize = 5;

/// An artist as the panel shows them (decided 2026-10-16, as Symfonium's): their albums and the albums they appear on,
/// newest first, and their tracks album by album as a playlist's rows, with the owner's plays (Top tracks; what Play
/// plays).
#[derive(Debug, Serialize)]
pub struct ArtistView {
    pub source: String,
    pub name: String,
    pub albums: Vec<source::Album>,
    pub appears_on: Vec<source::Album>,
    pub rows: Vec<super::playlists::PlaylistRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favorite: Option<bool>,
    /// Their MusicBrainz id, where Cratarr's own library knows it (0.9.8: Discogs' profile by MusicBrainz's link).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mbid: Option<String>,
    /// Their id for life (C55, `ar-<n>`): Cratarr's own artist, the one asked for by id, else the first of the name asked for.
    /// None: a name no artist has, or another server's artist.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Their number among the artists of their name (C55): the first 1, the next 2, for life.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<i64>,
    /// Another artist has their name (C55): where a view shows two of it, each shows its number, "Rone (2)".
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub namesakes: bool,
}

/// Cratarr's own artist an artist view asks for (C55), and their works in the library, as `account` sees them (`related::asked`):
/// by their id for life, that artist, though another has the name; by name, the first of it. One with no albums here (records
/// not had alone, or nothing yet) is a page too, with none. None: by name, a name no artist has. An id no artist has is no
/// longer known where it was forgotten (`retention::unknown_artist`), else no artist.
async fn own_artist(served: &Arc<crate::serve::Serve>, account: &str, id: Option<&str>, name: &str) -> Result<Option<(super::related::Asked, source::ArtistWorks)>, ToggleError> {
    let (s, i, n) = (served.clone(), id.map(str::to_string), name.to_string());
    let asked = tokio::task::spawn_blocking(move || super::related::asked(&s, i.as_deref(), &n)).await.map_err(anyhow::Error::from).and_then(|r| r).map_err(ToggleError::from)?;
    let Some(asked) = asked else {
        let Some(id) = id else { return Ok(None) };
        let (c, i) = (served.catalogue().clone(), id.to_string());
        let gone = tokio::task::spawn_blocking(move || super::retention::unknown_artist(&c, &i)).await.ok().flatten();
        return Err(gone.unwrap_or_else(|| ToggleError::Refused("That is not an artist.".into())));
    };
    let works = crate::source::cratarr::artist_works_of(served, account, &asked.id).await.map_err(|e| {
        tracing::warn!("artist {}: {e:#}", asked.id);
        ToggleError::Refused("The server could not read this artist.".into())
    })?;
    Ok(Some((asked, works)))
}

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
    /// When it came and when it was last played (epoch seconds), and how often: the panel's About; absent where the
    /// source cannot say.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub played: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plays: Option<u32>,
    /// What else is known of it (a Bandcamp album's page, and Discover's suggestion of it): each left out when unknown.
    #[serde(flatten)]
    pub more: AlbumMore,
}

/// An album's further facts, beyond what its tracks say (0.9.2; the owner, 2026-10-25: a Bandcamp album's panel "as much
/// as our own panel").
#[derive(Debug, Clone, Default, Serialize)]
pub struct AlbumMore {
    /// When it came out ("2019-05-17").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub released: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Its description, and its credits, as its page writes them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits: Option<String>,
    /// Its tags that are not among its genres.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Its words by kind, with their sources, and its tracks' own (decision 0097, the one store): Cratarr's own
    /// library's records.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub words: Option<super::words::store::Words>,
    /// Where it is elsewhere: its own page first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<super::discover::Link>,
    /// How many listen to it on Last.fm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listeners: Option<u64>,
    /// What a Want of a Bandcamp album sends (W1): Cratarr's record that holds its page (`al-<n>`), else the release group its page
    /// was found by. A record not had's page (C54): the record itself, and its first release group.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_group: Option<String>,
    /// A record not had (C54): its page from the catalogue, its rows its list's, none of them playable.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub not_had: bool,
    /// Its list is being read (C54): none kept yet, so its rows come when the page asks again.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub looking: bool,
    /// The record that has the album by the one had rule (C54, R13), where it is another (`al-<n>`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub have: Option<String>,
    /// The account's newest request of it, whatever its status, as the Requests page shows it (C54).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<super::requests::View>,
    /// Its labels (C54), each by its id for life.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<LabelOf>,
    /// Its cover's address (C54): its own, else its release group's, else its Bandcamp page's (`cover_cache::addresses_of`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    /// A retired record's (OQ16): when it went from Music, "Was in Music until <date>".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub was_until: Option<i64>,
    /// Its Bandcamp album page (Q34): its preview plays through the Bandcamp source, as before.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bandcamp: Option<String>,
}

/// A record's label as its page names it (C54): its id for life (`la-<n>`), its name, and its catalogue number there.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LabelOf {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catno: Option<String>,
}

/// A record's labels as its view names them (C54, G4): a record not had's page and an owned album's alike. Blocking.
fn labels_of(cat: &crate::catalogue::Catalogue, record: i64) -> Result<Vec<LabelOf>> {
    Ok(cat.record_labels(record)?.into_iter().map(|(l, catno)| LabelOf { id: crate::serve::ids::label(l.id), name: l.name, catno: Some(catno).filter(|c| !c.is_empty()) }).collect())
}

/// What Cratarr's album answers where the served library cannot read it (C54, `record_page`).
enum RecordPage {
    /// A record not had: its own page (`AlbumMore::not_had`).
    NotHad(Box<AlbumView>),
    /// Merged into a record that is had (G3): that record's id, whose usual view answers.
    Held(String),
}

/// The page of Cratarr's record `record` from the catalogue alone (C54), merges followed, read on readers: it never waits on the
/// writer (Z11). A record not had is opened at `at` (RT01: kept six months from then; not while the writer is busy), and shows:
/// - its names, year, genres and release date;
/// - its rows from its counting list as kept, else from the first kept in the design's order, none of them playable; with none
///   kept and a reference to read one by, `looking`;
/// - who has it (the one had rule's holder), its labels, its cover's address, its Bandcamp page (Q34), and when it went from Music
///   where it was retired (OQ16).
///
/// One merged into a record had is `Held`; a record had itself, or no such record, none: answered as before. Blocking.
fn record_page(cat: &crate::catalogue::Catalogue, record: i64, at: i64) -> Result<Option<RecordPage>> {
    use crate::catalogue::identity::COUNTING_ORDER;
    use crate::serve::ids;
    let Some(q) = cat.requested(&[record])?.remove(&record) else { return Ok(None) };
    let album = ids::record(q.row.id);
    if q.row.had {
        return Ok((q.row.id != record).then_some(RecordPage::Held(album)));
    }
    cat.opened(q.row.id, at)?;
    let kept = cat.reference_tracks(q.row.id)?;
    let listed = |source: &str| kept.iter().find(|(s, l)| s == source && !l.is_empty()).map(|(_, l)| l);
    let list = q.row.counting.as_deref().and_then(listed).or_else(|| COUNTING_ORDER.iter().find_map(|&s| listed(s)));
    let rows: Vec<super::playlists::PlaylistRow> = list
        .into_iter()
        .flatten()
        .map(|t| super::playlists::PlaylistRow {
            entry: format!("{album}/{}.{}", t.disc, t.position),
            file: t.title.clone(),
            id: None,
            name: Some(t.title.clone()),
            album: Some(q.row.title.clone()),
            duration_secs: t.length_ms.map(|ms| ms as f64 / 1000.0),
            disc: Some(t.disc),
            track: Some(t.position),
            album_id: Some(album.clone()),
            missing: true,
            ..Default::default()
        })
        .collect();
    let looking = list.is_none() && q.refs.iter().any(|(s, ..)| COUNTING_ORDER.contains(&s.as_str()));
    let labels = labels_of(cat, q.row.id)?;
    let cover = super::cover_cache::addresses_of(q.row.cover.as_deref(), q.groups.first().map(String::as_str), q.bandcamp.as_deref()).into_iter().next();
    let id = q.row.id;
    let row = q.row;
    Ok(Some(RecordPage::NotHad(Box::new(AlbumView {
        source: crate::source::cratarr::KIND.id.to_string(),
        id: album.clone(),
        title: row.title,
        artist: Some(row.artist).filter(|a| !a.is_empty()),
        year: row.year.and_then(|y| u32::try_from(y).ok()).filter(|y| *y > 0),
        genres: row.genres.into_iter().take(GENRES_SHOWN).collect(),
        format: None,
        bitrate_kbps: None,
        rows,
        added: None,
        played: None,
        plays: None,
        more: AlbumMore {
            released: row.released,
            record: Some(album),
            release_group: q.groups.into_iter().next(),
            not_had: true,
            looking,
            have: q.holder.filter(|h| *h != id).map(ids::record),
            labels,
            cover,
            was_until: row.retired_at,
            bandcamp: crate::bandcamp::shown(q.bandcamp),
            ..AlbumMore::default()
        },
    }))))
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
    /// The last state worked out: when, for whom and which source followed (`StateKey`), and it. Whoever asks the same
    /// within `STATE_REUSE` gets it,
    /// and one asks at a time: the pages (and the live channel) asking often, or a page caught in a loop, cost the media
    /// server one question at a time at most (2026-10-14: one tab's loop of 200 a second took it down).
    state_kept: tokio::sync::Mutex<Option<(std::time::Instant, StateKey, StateView)>>,
}

/// A state is its account's (their player, their favourites: decision 0051) for the source followed.
type StateKey = (Option<String>, Option<String>);

fn state_key(served: Option<&crate::serve::Serve>, want: Option<&str>) -> StateKey {
    (served.and_then(|s| s.current_user()), want.map(str::to_string))
}

/// How long a state worked out is answered again.
const STATE_REUSE: std::time::Duration = std::time::Duration::from_secs(1);

impl PlayingService {
    pub(super) fn new(lib: Arc<Library>, playlists: Arc<PlaylistService>) -> Self {
        Self { lib, playlists, info: Mutex::new(None), state_kept: tokio::sync::Mutex::new(None) }
    }

    /// What plays and which playlists hold it: as worked out within the last second, else afresh.
    pub async fn state(&self, want: Option<&str>) -> Result<StateView> {
        let key = state_key(self.playlists.served().as_deref(), want);
        let mut kept = self.state_kept.lock().await;
        if let Some((at, k, view)) = kept.as_ref()
            && *k == key
            && at.elapsed() < STATE_REUSE
        {
            return Ok(view.clone());
        }
        let view = self.work_out_state(want).await?;
        *kept = Some((std::time::Instant::now(), key, view.clone()));
        Ok(view)
    }

    /// After a change (a favourite, a playlist's membership): worked out afresh, and kept for whoever asks next.
    async fn fresh_state(&self, want: Option<&str>) -> Result<StateView> {
        let mut kept = self.state_kept.lock().await;
        let view = self.work_out_state(want).await?;
        *kept = Some((std::time::Instant::now(), state_key(self.playlists.served().as_deref(), want), view.clone()));
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
    /// plays and what comes next; tracks outside the library are left out (decision 0019). A track the queue plays
    /// twice is in it twice when `repeats` (the page asked: Keep repeats), else once (decision 0025, amended
    /// 2026-10-16).
    pub async fn playlist_from_queue(&self, want: Option<&str>, name: &str, ids: Option<Vec<String>>, repeats: bool) -> Result<FromQueue, ToggleError> {
        // Bandcamp's previews are never in a playlist (decision 0071); among a server's tracks they are left out.
        if want == Some(crate::bandcamp::SOURCE) {
            return Err(ToggleError::Refused(crate::bandcamp::NOT_KEPT.into()));
        }
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
        let folders = self.lib.catalogue().folders();
        let mut entries = Vec::new();
        let mut left_out = 0;
        for id in &ids {
            let entry = match s.track(id).await {
                Ok(t) => active.line(&folders, &t),
                Err(_) => None,
            };
            match entry {
                // A track twice in the queue: twice in the playlist when asked to keep repeats, else once.
                Some(e) if repeats || !entries.contains(&e) => entries.push(e),
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

    /// A track a player heard (decision 0064): a row in the account's log, and the play counted when it counts. Cratarr's
    /// own library's (a track of a server no longer on, by the file the catalogue links it to); another server's
    /// counts there as before, unlogged.
    pub async fn hear(&self, e: &super::plays::PlayEvent) -> Result<(), ToggleError> {
        // Never a play (decision 0071): answered as taken, so the player does not send it again.
        if crate::bandcamp::is_preview(&e.track) {
            return Ok(());
        }
        let own = if e.source == crate::source::cratarr::KIND.id {
            Some(e.track.clone())
        } else {
            let (cat, src, item) = (self.lib.catalogue(), e.source.clone(), e.track.clone());
            let gone = self.find(&e.source, &e.track).await.is_err();
            if gone { tokio::task::spawn_blocking(move || cat.track_of_source(&src, &item)).await.ok().flatten().map(crate::serve::ids::song) } else { None }
        };
        let Some(song) = own else {
            // A server switched off or removed, its track not linked to a file of Cratarr's own: nowhere to count it,
            // now or later, so taken. Only a server that is on but away leaves a play for later.
            if !self.lib.live().iter().any(|a| a.cfg.id == e.source && a.cfg.enabled) {
                tracing::debug!("a play of {}/{} is dropped: its server is off", e.source, e.track);
                return Ok(());
            }
            // Another server's track: only a play that counts reaches it.
            return if e.counts.unwrap_or(true) { self.count_play_at(&e.source, &e.track, Some(e.at / 1000)).await } else { Ok(()) };
        };
        let s = self.playlists.served().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        let user = s.current_user().ok_or_else(|| ToggleError::Refused("No one is signed in.".into()))?;
        let mut heard = e.heard();
        // Heard while its player followed another listener (the owner, 2026-10-30): history, not taste.
        heard.followed = e.player.as_deref().is_some_and(|p| super::PlayOn::instance().is_some_and(|on| on.was_following(p, heard.at)));
        crate::serve::queues::record(&s, &user, &song, &heard).await.map_err(|f| ToggleError::CannotPlay(format!("{f:#}")))?;
        if e.counts.unwrap_or(true) {
            *self.info.lock().unwrap_or_else(|p| p.into_inner()) = None;
        }
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
        let folders = self.lib.catalogue().folders();
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
                    asked_by: u.asked_by,
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
        let entry = active.line(&self.lib.catalogue().folders(), &playing.track).ok_or(ToggleError::NotInLibrary)?;
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
        let entry = a.line(&self.lib.catalogue().folders(), &track);
        let (favorite, info) = tokio::join!(s.favorite(id), self.info(&s, source_id, id));
        let playlists = self.playlists.view(entry.as_deref()).await?;
        Ok(TrackStateView { addable: entry.is_some(), album_id: track.album_id.clone(), favorite: favorite.unwrap_or(None), playlists, info })
    }

    /// Puts a given track in the playlist or takes it out, as `toggle` does for what the server's player plays.
    pub async fn track_toggle(&self, source_id: &str, id: &str, playlist_id: &str, member: Option<bool>) -> Result<TrackStateView, ToggleError> {
        if source_id == crate::bandcamp::SOURCE {
            return Err(ToggleError::Refused(crate::bandcamp::NOT_KEPT.into()));
        }
        let (a, _, track) = self.find(source_id, id).await?;
        self.lib.store()?;
        let entry = a.line(&self.lib.catalogue().folders(), &track).ok_or(ToggleError::NotInLibrary)?;
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
        s.set_album_favorite(album_id, on).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
    }

    /// An album's rating on a connected server, 0 to 5; `None` when the server keeps none.
    pub async fn album_rating(&self, source_id: &str, album_id: &str) -> Result<Option<u8>, ToggleError> {
        if !crate::covers::valid_item_id(album_id) {
            return Err(ToggleError::Refused("That is not an album.".into()));
        }
        let (_, s) = self.source(source_id)?;
        s.album_rating(album_id).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
    }

    /// Rates an album, 0 (none) to 5.
    pub async fn set_album_rating(&self, source_id: &str, album_id: &str, stars: u8) -> Result<u8, ToggleError> {
        if !crate::covers::valid_item_id(album_id) {
            return Err(ToggleError::Refused("That is not an album.".into()));
        }
        let (_, s) = self.source(source_id)?;
        s.set_album_rating(album_id, stars.min(5)).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))?;
        Ok(stars.min(5))
    }

    /// The favourite tracks of a connected server (their ids): Home's Favourite tracks.
    pub async fn favorite_tracks(&self, source_id: &str) -> Result<Vec<String>, ToggleError> {
        let (_, s) = self.source(source_id)?;
        s.favorite_tracks().await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
    }

    /// The favourite albums of a connected server (their ids): the Library's Favourites only, the album panel's heart.
    pub async fn favorite_albums(&self, source_id: &str) -> Result<Vec<String>, ToggleError> {
        let (_, s) = self.source(source_id)?;
        s.favorite_albums().await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
    }

    /// The favourite artists of a connected server, by name: Artists' Favourites only (0.9.15-1).
    pub async fn favorite_artists(&self, source_id: &str) -> Result<Vec<String>, ToggleError> {
        let (_, s) = self.source(source_id)?;
        s.favorite_artists().await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
    }

    /// How the owner used each album of a connected server (plays, last played, rating): the Library's orders and
    /// filters.
    pub async fn album_uses(&self, source_id: &str) -> Result<std::collections::BTreeMap<String, source::AlbumUse>, ToggleError> {
        let (_, s) = self.source(source_id)?;
        s.album_uses().await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
    }

    /// An album of a connected server, for its view in the panel.
    /// Several albums' tracks in one call (0.9.4): the albums in the order given (`(source, album)`), each one's in play
    /// order. A source read once for all its albums; an album, or a source, that cannot be read left out.
    pub async fn tracks_of_albums(&self, albums: &[(String, String)]) -> Vec<AlbumsTrack> {
        let mut by_source: Vec<(&str, Vec<String>)> = Vec::new();
        for (source, id) in albums.iter().filter(|(_, id)| crate::covers::valid_item_id(id)) {
            match by_source.iter_mut().find(|(s, _)| *s == source.as_str()) {
                Some((_, ids)) => ids.push(id.clone()),
                None => by_source.push((source.as_str(), vec![id.clone()])),
            }
        }
        let mut of: std::collections::HashMap<(String, String), Vec<source::AlbumTrack>> = std::collections::HashMap::new();
        for (source_id, ids) in by_source {
            let tracks = match self.source(source_id) {
                Ok((_, s)) => s.tracks_of_albums(&ids).await,
                Err(e) => Err(anyhow::anyhow!("{e}")),
            };
            match tracks {
                Ok(t) => t.into_iter().for_each(|(album, t)| of.entry((source_id.to_string(), album)).or_default().push(t)),
                Err(e) => tracing::warn!("{source_id}: {} albums' tracks not read: {e:#}", ids.len()),
            }
        }
        albums
            .iter()
            .filter_map(|k| Some((k, of.remove(k)?)))
            .flat_map(|((source, album), tracks)| {
                tracks.into_iter().map(move |t| AlbumsTrack {
                    source: source.clone(),
                    id: t.id,
                    name: t.title,
                    artist: t.artist,
                    album: t.album,
                    album_id: album.clone(),
                    duration_secs: t.duration_secs,
                })
            })
            .collect()
    }

    pub async fn album_view(&self, source_id: &str, album_id: &str) -> Result<AlbumView, ToggleError> {
        if !crate::covers::valid_item_id(album_id) {
            return Err(ToggleError::Refused("That is not an album.".into()));
        }
        let (_, s) = self.source(source_id)?;
        // The server's own words (a request and its status) go to the log, not the page. A record not had that was forgotten
        // (RT01) says so, by its tombstone.
        let tracks = match s.tracks_of_album(album_id).await {
            Ok(tracks) => tracks,
            Err(e) => {
                if source_id == crate::source::cratarr::KIND.id {
                    let (c, id) = (self.lib.catalogue(), album_id.to_string());
                    if let Ok(Some(gone)) = tokio::task::spawn_blocking(move || super::retention::unknown_album(&c, &id)).await {
                        return Err(gone);
                    }
                    // A record not had is no album of the library: its page answers from the catalogue (C54, `record_view`).
                    if crate::serve::ids::record_number(album_id).is_some() {
                        tracing::debug!("{source_id}: album {album_id}: {e:#}");
                        return Err(ToggleError::Refused("The server could not read this album.".into()));
                    }
                }
                tracing::warn!("{source_id}: album {album_id}: {e:#}");
                return Err(ToggleError::Refused("The server could not read this album.".into()));
            }
        };
        let first = tracks.first();
        let (title, artist, year) =
            (first.and_then(|t| t.album.clone()).unwrap_or_default(), first.and_then(|t| t.album_artist.clone()), first.and_then(|t| t.year));
        let (c, sid, ids) = (self.lib.catalogue(), source_id.to_string(), tracks.iter().map(|t| t.id.clone()).collect::<Vec<_>>());
        let per_track = tokio::task::spawn_blocking(move || c.genres_of_ids(&sid, &ids)).await.unwrap_or_default();
        let genres = crate::crates::top_genres(per_track.iter().map(|g| g.as_slice()), GENRES_SHOWN);
        // What the source knows of the files (Cratarr's own library does; a source that cannot say leaves them out).
        let details = s.album_summary(album_id).await.ok().flatten();
        let (format, bitrate_kbps) = details.map_or((None, None), |d| (d.format, d.bitrate_kbps));
        let mut rows: Vec<super::playlists::PlaylistRow> = tracks
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
                    ..Default::default()
                }
            })
            .collect();
        // Other versions (0.10): the tracks whose work the library holds another recording of. Works and movements
        // (0.12, decision 0086): each track's work, with the work that holds it, so the page sets a work's heading over
        // its movements.
        if source_id == crate::source::cratarr::KIND.id
            && let Some(served) = crate::serve::instance()
            && let Ok(Ok(ix)) = tokio::task::spawn_blocking(move || super::classical::index(&served)).await
        {
            for r in rows.iter_mut() {
                let Some(id) = r.id.as_deref() else { continue };
                if ix.has_versions(id) {
                    r.has_versions = Some(true);
                }
                r.work = ix.row_work(id);
            }
        }
        let listening = s.album_listening(album_id).await.unwrap_or_default();
        // Its labels (G4), as a record not had's page shows them: each a chip that opens the label's page.
        let labels = match crate::serve::ids::record_number(album_id).filter(|_| source_id == crate::source::cratarr::KIND.id) {
            Some(record) => {
                let c = self.lib.catalogue();
                tokio::task::spawn_blocking(move || labels_of(&c, record)).await.ok().and_then(Result::ok).unwrap_or_default()
            }
            None => Vec::new(),
        };
        // Its words, from the one store (decision 0097).
        let words = match crate::serve::instance() {
            Some(served) if source_id == crate::source::cratarr::KIND.id => {
                let id = album_id.to_string();
                tokio::task::spawn_blocking(move || super::words::stored(&served).map(|s| s.records.get(&id).cloned())).await.ok().and_then(Result::ok).flatten()
            }
            _ => None,
        };
        Ok(AlbumView {
            source: source_id.to_string(),
            id: album_id.to_string(),
            title,
            artist,
            year,
            genres,
            format,
            bitrate_kbps,
            rows,
            added: listening.added,
            played: listening.played,
            plays: listening.plays,
            more: AlbumMore { words, labels, ..AlbumMore::default() },
        })
    }

    /// What Cratarr's album `album` answers where the served library cannot read it (C54): a record not had, its own page from the
    /// catalogue (`record_page`), its list asked for where none is kept (`references::read_soon`); one merged into a record had,
    /// that one's view. The page's path alone: opening it marks the record opened. None: no record of the catalogue's, or one had
    /// that the library does not serve now (answered as before).
    pub async fn record_view(&self, album: &str) -> Result<Option<AlbumView>, ToggleError> {
        let Some(record) = crate::serve::ids::record_number(album) else { return Ok(None) };
        let cat = self.lib.catalogue();
        let c = cat.clone();
        let page = match tokio::task::spawn_blocking(move || record_page(&c, record, super::moment::now_secs())).await {
            Ok(Ok(page)) => page,
            Ok(Err(e)) => {
                tracing::warn!("record {album}: {e:#}");
                None
            }
            Err(e) => {
                tracing::warn!("record {album}: {e}");
                None
            }
        };
        match page {
            Some(RecordPage::NotHad(mut view)) => {
                if view.more.looking {
                    let end = crate::serve::ids::record_number(&view.id).unwrap_or(record);
                    view.more.looking = super::references::read_soon(&cat, end, super::moment::now_secs());
                }
                Ok(Some(*view))
            }
            Some(RecordPage::Held(end)) => self.album_view(crate::source::cratarr::KIND.id, &end).await.map(Some),
            None => Ok(None),
        }
    }

    /// An artist of a connected server, for their view in the panel (by name, as the albums credit them).
    /// `without`: the libraries switched off (`source/library` keys): their albums and tracks left out, as every list
    /// leaves them (the owner, 2026-11-02).
    /// Cratarr's own artist goes by their id for life (C55, `id`): one asked for by name is the first of it, and the view says who.
    pub async fn artist_view(&self, source_id: &str, name: &str, id: Option<&str>, without: &[String]) -> Result<ArtistView, ToggleError> {
        let name = name.trim();
        let own = source_id == crate::source::cratarr::KIND.id;
        let id = id.map(str::trim).filter(|i| own && !i.is_empty());
        if name.is_empty() && id.is_none() {
            return Err(ToggleError::Refused("That is not an artist.".into()));
        }
        let (_, s) = self.source(source_id)?;
        let found = match self.playlists.served() {
            Some(served) if own => {
                let account = served.current_user().ok_or_else(|| ToggleError::Refused("No one is signed in.".into()))?;
                own_artist(&served, &account, id, name).await?
            }
            _ => None,
        };
        let (asked, works) = match found {
            Some((asked, works)) => (Some(asked), Some(works)),
            None => (
                None,
                s.artist_works(name).await.map_err(|e| {
                    tracing::warn!("{source_id}: artist {name}: {e:#}");
                    ToggleError::Refused("The server could not read this artist.".into())
                })?,
            ),
        };
        let name = asked.as_ref().map_or(name, |a| a.name.as_str());
        let w = works.ok_or_else(|| ToggleError::Refused("This server cannot show artists.".into()))?;
        let mut rows: Vec<super::playlists::PlaylistRow> = w
            .tracks
            .into_iter()
            .map(|a| {
                let t = a.track;
                let entry = t.path.unwrap_or_else(|| t.id.clone());
                super::playlists::PlaylistRow {
                    file: entry.rsplit('/').next().unwrap_or(&entry).to_string(),
                    entry,
                    id: Some(t.id),
                    name: Some(t.title),
                    artist: t.artist,
                    album: t.album,
                    duration_secs: t.duration_secs,
                    album_artist: t.album_artist,
                    year: t.year.map(|y| y as i32),
                    play_count: Some(a.plays),
                    last_played: a.played,
                    album_id: Some(a.album_id),
                    missing: false,
                    ..Default::default()
                }
            })
            .collect();
        // Their tempo (decision 0035), as a playlist's rows have it: the catalogue's (the BPM tag, else the analysis').
        let (c, sid, ids) = (self.lib.catalogue(), source_id.to_string(), rows.iter().filter_map(|r| r.id.clone()).collect::<Vec<_>>());
        if let Ok(Ok(facts)) = tokio::task::spawn_blocking(move || c.source_rows(&sid, &ids)).await {
            for (row, f) in rows.iter_mut().filter(|r| r.id.is_some()).zip(facts) {
                row.bpm = f.bpm;
            }
        }
        // Their own references' first, else the library's (C55, `related::asked`).
        let mbid = asked.as_ref().and_then(|a| a.mbid.clone());
        // Classical (0.10), as the Library's albums say it.
        let (mut albums, mut appears_on) = (w.albums, w.appears_on);
        // Of the libraries switched on: an album of one switched off goes, and its tracks with it.
        if !without.is_empty() {
            // Cratarr's own records by their copies: its list names the copy that plays, and that one can be the library off.
            let held = match self.playlists.served() {
                Some(served) if source_id == crate::source::cratarr::KIND.id => tokio::task::spawn_blocking(move || served.library.album_folders()).await.ok().and_then(Result::ok).unwrap_or_default(),
                _ => Default::default(),
            };
            let folders_off: std::collections::HashSet<i64> = self.lib.folders_off(without).into_iter().collect();
            let off = |a: &source::Album| match held.get(&a.id) {
                Some(folders) => !crate::crates::on_for(folders, &folders_off),
                None => a.library.as_ref().is_some_and(|l| without.contains(&format!("{source_id}/{l}"))),
            };
            let gone: std::collections::HashSet<String> = albums.iter().chain(&appears_on).filter(|a| off(a)).map(|a| a.id.clone()).collect();
            albums.retain(|a| !off(a));
            appears_on.retain(|a| !off(a));
            rows.retain(|r| r.album_id.as_ref().is_none_or(|a| !gone.contains(a)));
        }
        super::classical::tell(source_id, &mut albums).await;
        super::classical::tell(source_id, &mut appears_on).await;
        super::file_facts::tell(source_id, &mut albums);
        super::file_facts::tell(source_id, &mut appears_on);
        Ok(ArtistView {
            source: source_id.to_string(),
            name: name.to_string(),
            albums,
            appears_on,
            rows,
            favorite: w.favorite,
            mbid,
            id: asked.as_ref().map(|a| a.id.clone()),
            number: asked.as_ref().and_then(|a| a.number),
            namesakes: asked.as_ref().is_some_and(|a| a.namesakes),
        })
    }

    /// Cratarr's own artist asked for by their id for life (C55): the library, the asking account and the id. None: none asked
    /// for, or another server's artist.
    fn own_artist_id(&self, source_id: &str, id: Option<&str>) -> Result<Option<(Arc<crate::serve::Serve>, String, String)>, ToggleError> {
        let Some(id) = id.map(str::trim).filter(|i| !i.is_empty() && source_id == crate::source::cratarr::KIND.id) else { return Ok(None) };
        if crate::serve::ids::artist_number(id).is_none() {
            return Err(ToggleError::Refused("That is not an artist.".into()));
        }
        let served = self.playlists.served().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
        let account = served.current_user().ok_or_else(|| ToggleError::Refused("No one is signed in.".into()))?;
        Ok(Some((served, account, id.to_string())))
    }

    /// Makes an artist a favourite or not (the artist panel's heart): Cratarr's own by their id for life where one is given (C55).
    pub async fn artist_favorite(&self, source_id: &str, name: &str, id: Option<&str>, on: bool) -> Result<bool, ToggleError> {
        let (_, s) = self.source(source_id)?;
        if let Some((served, account, id)) = self.own_artist_id(source_id, id)? {
            let set = tokio::task::spawn_blocking(move || crate::source::cratarr::set_artist_favorite_of(&served, &account, &id, on));
            return set.await.map_err(anyhow::Error::from).and_then(|r| r).map_err(|e| ToggleError::CannotPlay(format!("{e:#}")));
        }
        s.set_artist_favorite(name.trim(), on).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
    }

    /// Whether an artist is a favourite (the heart on their chip); `None` where the server keeps no favourite artists. Cratarr's
    /// own by their id for life where one is given (C55).
    pub async fn artist_is_favorite(&self, source_id: &str, name: &str, id: Option<&str>) -> Result<Option<bool>, ToggleError> {
        let (_, s) = self.source(source_id)?;
        if let Some((served, account, id)) = self.own_artist_id(source_id, id)? {
            let read = tokio::task::spawn_blocking(move || crate::source::cratarr::artist_favorite_of(&served, &account, &id));
            return read.await.map_err(anyhow::Error::from).and_then(|r| r).map(Some).map_err(|e| ToggleError::CannotPlay(format!("{e:#}")));
        }
        s.artist_favorite(name.trim()).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))
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
            Some((cat.folders().get(folder)?.clone(), rel))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn play(source: &str, track: &str) -> super::super::plays::PlayEvent {
        serde_json::from_value(serde_json::json!({ "id": "p", "source": source, "track": track, "at": 1_792_000_000_000_i64 })).unwrap()
    }

    #[tokio::test]
    async fn a_play_that_can_never_count_is_taken_and_one_of_a_server_away_is_left_for_later() {
        let d = std::env::temp_dir().join(format!("cratarr-hear-{}", std::process::id()));
        std::fs::create_dir_all(d.join(crate::playlists::PLAYLIST_DIR)).unwrap();
        let source = |id: &str, enabled| crate::settings::SourceConfig { id: id.into(), kind: "jellyfin".into(), enabled, libraries: Default::default(), values: Default::default() };
        let lib = super::super::Library::new(None, Some(d.to_string_lossy().into_owned()), vec![source("old", false), source("away", true)], Default::default(), None, Arc::new(crate::userdata::UserData::open(None).unwrap())).await.unwrap();
        let journal = Arc::new(crate::journal::Journal::open(None).unwrap());
        let state = crate::sync_state::SyncState::open(None).unwrap();
        let sync = Arc::new(super::super::sync::SyncService::new(lib.clone(), state, journal.clone()));
        let playing = PlayingService::new(lib.clone(), Arc::new(PlaylistService::new(lib, sync, journal, None)));
        assert!(playing.hear(&play(crate::bandcamp::SOURCE, &crate::bandcamp::track_id("https://a.bandcamp.com/album/b", 1))).await.is_ok(), "a preview");
        assert!(playing.hear(&play("old", "3d5336410f0c2dbfca17dd0dbc334d23")).await.is_ok(), "a server switched off");
        assert!(playing.hear(&play("gone", "3d5336410f0c2dbfca17dd0dbc334d23")).await.is_ok(), "a server removed");
        assert!(playing.hear(&play("away", "3d5336410f0c2dbfca17dd0dbc334d23")).await.is_err(), "a server on but away");
        std::fs::remove_dir_all(&d).ok();
    }

    // ---- C54: the page of a record not had

    use crate::catalogue::Catalogue;
    use crate::catalogue::identity::RefTrack;
    use crate::catalogue::records::Ref;
    use crate::serve::ids;

    /// The pages' clock in the tests, and a month.
    const NOW: i64 = 1_800_000_000;
    const MONTH: i64 = 30 * 86_400;

    /// A record not had's page, as `record_page` answers it now.
    fn page(cat: &Catalogue, record: i64) -> AlbumView {
        match record_page(cat, record, NOW).unwrap() {
            Some(RecordPage::NotHad(v)) => *v,
            _ => panic!("al-{record} is no page of a record not had"),
        }
    }

    fn track(position: u32, title: &str) -> RefTrack {
        RefTrack { disc: 1, position, title: title.into(), length_ms: Some(200_000), recording: None }
    }

    /// C54: a record not had is a page from the catalogue: its names, year and date; its rows its counting list's (Bandcamp's here,
    /// though Discogs' comes first in the order), none of them playable; its labels; its cover's address. With no list kept it has
    /// no rows yet, and asks for one where a reference may give it; its cover is then its Bandcamp page's, and its page is kept for
    /// the preview (Q34).
    #[test]
    fn a_record_not_had_is_a_page_not_playable() {
        let _one = crate::app::switches::TEST_STATE.blocking_lock();
        let was = crate::app::switches::set_for_tests(crate::app::switches::BANDCAMP, true);
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let record = cat.record_for_tests("Night Walks", 0);
        let group = "00000000-0000-4000-8000-000000000194";
        assert!(cat.set_reference(&ids::record(record), &Ref::new("musicbrainz", "release-group", group), "link").unwrap());
        cat.db_for_tests().execute("UPDATE records SET year = 2019, released = '2019-05-17', genres = 'Ambient' WHERE id = ?1", [record]).unwrap();
        let label = cat.label_for("Low Tide Records").unwrap();
        cat.db_for_tests().execute("INSERT INTO record_labels (record, label, catno, how) VALUES (?1, ?2, 'LTR-012', 'listing')", [record, label]).unwrap();
        cat.set_reference_tracks(record, "discogs", &[track(1, "Other")]).unwrap();
        cat.set_reference_tracks(record, "bandcamp", &[track(1, "Lamplight"), track(2, "Morning")]).unwrap();
        cat.set_counting(record, Some("bandcamp"), false).unwrap();
        let v = page(&cat, record);
        assert_eq!((v.id.clone(), v.title.as_str(), v.artist.as_deref(), v.year), (ids::record(record), "Night Walks", Some("Glass Harbour"), Some(2019)));
        assert_eq!((v.genres.clone(), v.more.released.as_deref()), (vec!["Ambient".to_string()], Some("2019-05-17")));
        let rows: Vec<(Option<&str>, Option<u32>, Option<f64>)> = v.rows.iter().map(|r| (r.name.as_deref(), r.track, r.duration_secs)).collect();
        assert_eq!(rows, [(Some("Lamplight"), Some(1), Some(200.0)), (Some("Morning"), Some(2), Some(200.0))], "its counting list's");
        assert!(v.rows.iter().all(|r| r.id.is_none() && r.missing), "none of them plays");
        assert!(v.more.not_had && !v.more.looking);
        assert_eq!(v.more.cover, Some(format!("/api/covers/release-group/{group}")));
        assert_eq!(serde_json::to_value(&v.more.labels).unwrap(), serde_json::json!([{ "id": ids::label(label), "name": "Low Tide Records", "catno": "LTR-012" }]));
        assert_eq!((v.more.record.clone(), v.more.release_group.as_deref()), (Some(ids::record(record)), Some(group)), "what a Want sends");
        // No list kept, a Bandcamp page alone: no rows yet, its list asked for; the page's cover, and its preview.
        let at = "https://kitetheory.bandcamp.com/album/quiet-lanes";
        let bare = cat.record_for_tests("Quiet Lanes", 0);
        assert!(cat.set_reference(&ids::record(bare), &Ref::new("bandcamp", "album", at), "link").unwrap());
        let v = page(&cat, bare);
        assert!(v.rows.is_empty() && v.more.looking);
        assert_eq!(v.more.cover, Some(format!("/api/cover/bandcamp/{}", crate::bandcamp::track_id(at, 0))));
        crate::app::switches::set_for_tests(crate::app::switches::BANDCAMP, false);
        let hidden = page(&cat, bare).more.bandcamp;
        crate::app::switches::set_for_tests(crate::app::switches::BANDCAMP, was);
        assert_eq!((v.more.bandcamp.as_deref(), v.more.labels.len()), (Some(at), 0));
        assert_eq!(hidden, None, "G1: Bandcamp switched off, no preview offered");
    }

    /// C54 (G3): a record merged into another answers as that one: its page moves to the record it is now; merged into a record
    /// had, that record's usual view answers. A record had is the library's, no page.
    #[test]
    fn a_merged_record_moves() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (from, into) = (cat.record_for_tests("Two Rooms", 0), cat.record_for_tests("Two Rooms (Deluxe)", 0));
        cat.merge_records(from, into).unwrap();
        let v = page(&cat, from);
        assert_eq!((v.id, v.title), (ids::record(into), "Two Rooms (Deluxe)".to_string()));
        let (gone, had) = (cat.record_for_tests("Signal", 0), cat.record_for_tests("Signal", 2));
        cat.merge_records(gone, had).unwrap();
        assert!(matches!(record_page(&cat, gone, NOW).unwrap(), Some(RecordPage::Held(id)) if id == ids::record(had)), "its usual view");
        assert!(record_page(&cat, had, NOW).unwrap().is_none(), "a record had is the library's");
    }

    /// C54, RT01: opening a record's page marks it opened, never earlier than it was, and only it: it is kept six months from
    /// then, though no list showed it for longer.
    #[test]
    fn opening_a_record_marks_it_opened() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (opened, other) = (cat.record_for_tests("Undertow", 0), cat.record_for_tests("Open Water", 0));
        cat.db_for_tests().execute("UPDATE records SET seen_at = ?1", [NOW - 7 * MONTH]).unwrap();
        let at = |id: i64| cat.db_for_tests().query_row("SELECT opened_at FROM records WHERE id = ?1", [id], |r| r.get::<_, Option<i64>>(0)).unwrap();
        record_page(&cat, opened, NOW - MONTH).unwrap();
        assert_eq!(at(opened), Some(NOW - MONTH));
        record_page(&cat, opened, NOW - 2 * MONTH).unwrap();
        assert_eq!(at(opened), Some(NOW - MONTH), "never earlier than it was");
        assert_eq!(at(other), None, "only the record opened");
        assert_eq!(cat.forget_quiet(NOW, &Default::default()).unwrap(), [other], "the record opened stays");
    }

    /// Z11: a record not had's page never waits on the writer: while a build holds it, the page answers, its list's rows from a
    /// reader, and its opening's mark is skipped (a missed mark only shortens the keep by one visit); the writer free, it is marked.
    #[test]
    fn a_record_page_never_waits_on_the_writer() {
        let dir = std::env::temp_dir().join(format!("cratarr-page-writer-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let cat = Catalogue::open(Some(&dir.join("catalogue.sqlite")), Vec::new()).unwrap();
        let record = cat.record_for_tests("Night Walks", 0);
        cat.set_reference_tracks(record, "musicbrainz", &[track(1, "Lamplight")]).unwrap();
        let opened_at = || cat.db_for_tests().query_row("SELECT opened_at FROM records WHERE id = ?1", [record], |r| r.get::<_, Option<i64>>(0)).unwrap();
        let build = cat.writer_for_tests();
        let answered = std::thread::scope(|s| {
            let (tx, rx) = std::sync::mpsc::channel();
            let c = &cat;
            s.spawn(move || tx.send(record_page(c, record, NOW).map(|p| matches!(p, Some(RecordPage::NotHad(v)) if v.rows.len() == 1))).ok());
            let answered = rx.recv_timeout(std::time::Duration::from_secs(5));
            drop(build);
            answered
        });
        assert!(answered.expect("answered while a build holds the writer").unwrap(), "its page, its list's rows");
        assert_eq!(opened_at(), None, "the mark skipped");
        record_page(&cat, record, NOW).unwrap();
        assert_eq!(opened_at(), Some(NOW), "the writer free: marked");
        drop(cat);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// OQ16 (a): a retired record's page is a record not had's, what a Want sends with it, and says when it went from Music; one
    /// never had says nothing of the kind.
    #[test]
    fn a_retired_record_says_until_when() {
        const WENT: i64 = 1_790_000_000;
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let record = cat.record_for_tests("Harbour Lights", 2);
        cat.db_for_tests().execute("UPDATE tracks SET retired_at = ?2 WHERE record = ?1", [record, WENT]).unwrap();
        cat.db_for_tests().execute("UPDATE records SET retired_at = ?2 WHERE id = ?1", [record, WENT]).unwrap();
        let v = page(&cat, record);
        assert_eq!((v.more.not_had, v.more.was_until, v.more.record), (true, Some(WENT), Some(ids::record(record))));
        assert_eq!(page(&cat, cat.record_for_tests("Quiet Lanes", 0)).more.was_until, None, "one never had");
    }

    /// G4: an owned record's labels, as its view shows them (`labels_of`): each by its `la-` id, its name and its catalogue number
    /// where it has one, so the page shows chips that open the label's page, as for a record not had.
    #[test]
    fn an_owned_records_labels_open_their_pages() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let owned = cat.record_for_tests("Tide Tables", 2);
        let (low, kite) = (cat.label_for("Low Tide Records").unwrap(), cat.label_for("Kite Records").unwrap());
        for (label, catno) in [(low, "LTR-007"), (kite, "")] {
            cat.db_for_tests().execute("INSERT INTO record_labels (record, label, catno, how) VALUES (?1, ?2, ?3, 'files')", rusqlite::params![owned, label, catno]).unwrap();
        }
        let mut labels = serde_json::to_value(labels_of(&cat, owned).unwrap()).unwrap().as_array().cloned().unwrap();
        labels.sort_by_key(|l| l["name"].as_str().unwrap_or_default().to_string());
        assert_eq!(
            serde_json::Value::Array(labels),
            serde_json::json!([{ "id": ids::label(kite), "name": "Kite Records" }, { "id": ids::label(low), "name": "Low Tide Records", "catno": "LTR-007" }])
        );
    }

    /// C54 (R13): a record not had that another record has by the one had rule (here by its artist and title, nothing
    /// contradicting) says which; one nobody has says none.
    #[test]
    fn a_record_not_had_shows_who_has_it() {
        let cat = Catalogue::open(None, Vec::new()).unwrap();
        let (had, wanted) = (cat.record_for_tests("Tide Tables", 2), cat.record_for_tests("Tide Tables", 0));
        assert_eq!(page(&cat, wanted).more.have, Some(ids::record(had)));
        assert_eq!(page(&cat, cat.record_for_tests("Quiet Lanes", 0)).more.have, None, "nobody has it");
    }

    // ---- C55: an artist page for anyone, by id or name

    /// Cratarr's own library, in memory, its catalogue attached.
    fn own_library() -> Arc<crate::serve::Serve> {
        let served = crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        served.attach_catalogue(Arc::new(Catalogue::open(None, Vec::new()).unwrap()));
        served
    }

    /// C55: an artist with no albums here (a record not had of theirs alone) is a page all the same: by their id, their name and
    /// number, no albums, no rows. A forgotten id is no longer known; an id never given is no artist.
    #[tokio::test]
    async fn an_artist_with_no_albums_here_is_a_page() {
        let served = own_library();
        let cat = served.catalogue().clone();
        let artist = cat.artist_for("Paper Lantern").unwrap();
        let record = cat.record_for_tests("After Hours", 0);
        cat.db_for_tests().execute("INSERT INTO record_artists (record, artist, position, how) VALUES (?1, ?2, 0, 'listing')", [record, artist]).unwrap();
        let (asked, works) = own_artist(&served, "me", Some(ids::artist(artist).as_str()), "").await.unwrap().expect("a page");
        assert_eq!((asked.id, asked.name.as_str(), asked.number, asked.namesakes), (ids::artist(artist), "Paper Lantern", Some(1), false));
        assert!(works.albums.is_empty() && works.appears_on.is_empty() && works.tracks.is_empty(), "nothing here");
        assert_eq!(works.favorite, Some(false));
        let gone = cat.artist_for("Quiet Lanes").unwrap();
        let kept: std::collections::BTreeSet<i64> = [artist].into();
        assert_eq!(cat.forget_quiet_artists(super::super::moment::now_secs() + 2 * 86_400, &kept).unwrap(), [gone]);
        assert!(matches!(own_artist(&served, "me", Some(ids::artist(gone).as_str()), "").await, Err(ToggleError::Gone(_))), "no longer known");
        assert!(matches!(own_artist(&served, "me", Some("ar-999999"), "").await, Err(ToggleError::Refused(_))), "no artist");
    }

    /// C55: an artist asked for by name hands back the id it resolved: the first of the name, with their number and whether
    /// another has it, named as asked. By that other's id, the view is theirs. A name no artist has is no one's.
    #[tokio::test]
    async fn a_name_hands_back_its_id() {
        let served = own_library();
        let cat = served.catalogue().clone();
        let (first, second) = (cat.artist_for("Rone").unwrap(), cat.artist_apart("Rone").unwrap());
        let (asked, _) = own_artist(&served, "me", None, " rone ").await.unwrap().expect("the first of the name");
        assert_eq!((asked.id, asked.name.as_str(), asked.number, asked.namesakes), (ids::artist(first), "rone", Some(1), true));
        let (asked, _) = own_artist(&served, "me", Some(ids::artist(second).as_str()), "Rone").await.unwrap().expect("the second");
        assert_eq!((asked.id, asked.name.as_str(), asked.number), (ids::artist(second), "Rone", Some(2)), "by id, though another has the name");
        assert!(own_artist(&served, "me", None, "Nobody Here").await.unwrap().is_none());
    }
}
