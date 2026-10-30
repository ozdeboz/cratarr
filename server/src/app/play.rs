//! Playing a collection (the owner, 2026-10-23: "we should limit to maximum 500 tracks in our queue"): one operation
//! for every surface and every player. What plays is the server's choice, from the whole collection, not from whatever
//! the screen pressed had loaded (Unlisted's 10,000 tracks and more played the same few, its panel's first page).
//!
//! - **What:** a crate (its rules, its two toggles as the page shows them; a genre is the crate `genre:<name>`), a
//!   playlist, an album, an artist, tracks named one by one (a selection, a queue), the favourites, or the library.
//! - **How:** Play (the collection's order, its first [`QUEUE_MOST`], or from a given track on), Shuffle
//!   ([`QUEUE_MOST`] drawn evenly from all of it, then mixed; all of it when it is that small), Play next and Add to
//!   queue (in order, as Play takes them).
//! - **Where:** the answer always carries the tracks (the page's own player queues them itself); asked, the server
//!   also queues them on the server's player (as `/api/queue/add` and a playlist's Play reach it) or on one of
//!   Cratarr's players by name (Play On, decision 0031).
//!
//! The picking itself is the rules' (cratarr-rules, decision 0059), and [`choose`] is the one way to it: Play On's play
//! and add by name, a playlist's Play and the voice assistant's play by name go through it too.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{App, QueueTrack, ToggleError};
use crate::crates::Favourites;
use crate::source::{self, PlayHow};

/// What is played.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Collection {
    /// A crate, a genre's too (`genre:<name>`); its toggles as the page shows them (a tap may be on its way), else as
    /// saved; from the libraries not in `without` (the page's switched off, `source/library`), else from every one.
    Crate {
        id: String,
        #[serde(default)]
        favourites: Option<Favourites>,
        #[serde(default)]
        favourite_albums: Option<Favourites>,
        #[serde(default)]
        without: Vec<String>,
        /// The filter on it (decision 0061): conditions, unsaved, besides its own.
        #[serde(default)]
        filter: Vec<crate::crates::Condition>,
        /// The order its list shows (decision 0060), so Play plays as it is shown.
        #[serde(default)]
        order: super::CrateOrder,
        #[serde(default)]
        down: bool,
    },
    Playlist {
        id: String,
    },
    Album {
        source: String,
        id: String,
    },
    Artist {
        source: String,
        name: String,
    },
    Tracks {
        source: String,
        ids: Vec<String>,
    },
    /// The favourite tracks of the one who asks (of a player's owner, through Play On), less what they excluded from
    /// automatic picks (decision 0024, amended 2026-10-19).
    Favourites,
    /// Cratarr's own library, all of it, less what is excluded as for the favourites.
    Library,
}

pub use cratarr_rules::{How, QUEUE_MOST, Rng};

/// Where the server queues them itself: the server's player (the one that plays, or the one followed), or one of
/// Cratarr's players by its name.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum On {
    Server,
    Player(String),
}

#[derive(Debug, Deserialize)]
pub struct Request {
    pub what: Collection,
    pub how: How,
    /// From this track (its id): the collection's order from it on, or shuffled with it first.
    #[serde(default)]
    pub from: Option<String>,
    /// Left out: only the answer (the asker plays the tracks itself).
    #[serde(default)]
    pub on: Option<On>,
    /// The queue's name on a player of Cratarr's; else the collection's.
    #[serde(default)]
    pub name: Option<String>,
    /// At most this many (never more than [`QUEUE_MOST`]); left out: [`QUEUE_MOST`].
    #[serde(default)]
    pub limit: Option<usize>,
}

/// The tracks chosen, in queue order, as players hold them (`n`: each one's place in the collection, the order repeat
/// all's rounds follow), the one that plays first, how many the collection has that can be played, and the
/// player that took them when the server queued them.
#[derive(Debug, Serialize)]
pub struct Played {
    pub source: String,
    pub tracks: Vec<QueueTrack>,
    pub index: usize,
    pub total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player: Option<String>,
}

/// A generator for one play, seeded from the operating system (the rules take a seed: cratarr-rules).
pub fn rng() -> Rng {
    let mut b = [0u8; 8];
    let seed = match getrandom::fill(&mut b) {
        Ok(()) => u64::from_le_bytes(b),
        Err(_) => std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1),
    };
    Rng::seeded(seed)
}

/// A collection's playable tracks, in its order: the server their ids are on, and each one's id and, for lines of a
/// playlist or a crate, its line (its name is read only for the tracks chosen).
struct Candidates {
    source: String,
    ids: Vec<String>,
    lines: Option<Vec<String>>,
    /// Tracks that come with their names (an album's, an artist's).
    named: Option<Vec<QueueTrack>>,
    /// Songs of Cratarr's own library, named from it (the favourites, the library).
    own: bool,
    /// The collection's own name, for a queue on a player of Cratarr's.
    name: Option<String>,
    /// A playlist's lines that cannot be played (no file, or not known to the server yet).
    unknown: usize,
}

impl Candidates {
    fn of(source: String, ids: Vec<String>, name: Option<String>) -> Self {
        Self { source, ids, lines: None, named: None, own: false, name, unknown: 0 }
    }
}

/// Where a play starts: from a track (its id), or from a place in the collection (Play On's `index`).
#[derive(Debug, Clone, PartialEq)]
pub enum Start {
    Track(String),
    Place(usize),
}

/// What a play chose (see [`choose`]): the tracks in queue order (`n`: each one's place in the collection), the one
/// that plays first, how many the collection has that can be played and how many of its lines cannot, and its name.
#[derive(Debug)]
pub struct Chosen {
    pub source: String,
    pub tracks: Vec<QueueTrack>,
    pub index: usize,
    pub total: usize,
    pub unknown: usize,
    pub name: Option<String>,
}

/// The one way tracks are chosen for a play, whoever asks (the page, Play On's `play` and `add` by name, a playlist's
/// Play, a voice assistant's play by name): the collection's playable tracks, then the rules' pick (cratarr-rules), at
/// most `most` (at most [`QUEUE_MOST`]). `want`: the server to use, else the first connected; `owner`: whose favourites
/// and exclusions.
pub async fn choose(
    app: &Arc<App>,
    want: Option<&str>,
    what: &Collection,
    how: How,
    start: Option<Start>,
    most: usize,
    owner: Option<&str>,
) -> Result<Chosen, ToggleError> {
    let c = candidates(app, want, what, owner).await?;
    let total = c.ids.len();
    if total == 0 {
        return Err(ToggleError::CannotPlay("Nothing in it can be played.".into()));
    }
    let at = match start {
        Some(Start::Track(f)) => c.ids.iter().position(|id| *id == f),
        Some(Start::Place(p)) => Some(p),
        None => None,
    };
    let picked = cratarr_rules::pick(total, how, at, most.clamp(1, QUEUE_MOST), &mut rng());
    let (tracks, index) = tracks_of(app, &c, &picked.chosen, picked.index).await?;
    if tracks.is_empty() {
        return Err(ToggleError::CannotPlay("Nothing in it can be played.".into()));
    }
    Ok(Chosen { source: c.source, tracks, index, total, unknown: c.unknown, name: c.name })
}

/// Plays a collection (see the module's notes). `want`: the server to use (the page's), else the first connected.
pub async fn play(app: &Arc<App>, want: Option<&str>, r: Request) -> Result<Played, ToggleError> {
    let owner = app.served.as_ref().and_then(|s| s.current_user());
    let most = r.limit.unwrap_or(QUEUE_MOST);
    let c = choose(app, want, &r.what, r.how, r.from.map(Start::Track), most, owner.as_deref()).await?;
    let player = match r.on {
        None => None,
        Some(On::Server) => Some(on_server(app, &c.source, &c.tracks, c.index, r.how).await?.0),
        Some(On::Player(name)) => {
            let queue_name = r.name.or(c.name);
            on_player(app, &name, c.tracks.clone(), c.index, r.how, queue_name).await?;
            Some(name)
        }
    };
    Ok(Played { source: c.source, tracks: c.tracks, index: c.index, total: c.total, player })
}

/// A playlist played on the server's player (`POST /api/playlists/{id}/play`), through [`choose`] as every play.
pub async fn play_playlist(app: &Arc<App>, id: &str, how: How, want: Option<&str>) -> Result<super::PlayResult, ToggleError> {
    let c = match choose(app, want, &Collection::Playlist { id: id.to_string() }, how, None, QUEUE_MOST, None).await {
        Ok(c) => c,
        // Usually the catalogue is still matching files to the server (a first fill takes minutes): say so.
        Err(ToggleError::CannotPlay(_)) => return Err(ToggleError::CannotPlay(app.playlists.why_none(want).await?)),
        Err(e) => return Err(e),
    };
    let (player, sent) = on_server(app, &c.source, &c.tracks, c.index, how).await?;
    Ok(super::PlayResult { player, sent, total: c.total, unknown: c.unknown })
}

/// The playlist meant by what was said, as its tracks' ids on a server (decision 0015; `GET /api/playlists/by-name`):
/// the voice assistant plays them itself. At most `limit` (1 to [`super::playlists::PLAY_MAX`]: a Jellyfin player holds
/// no more), through [`choose`].
pub async fn by_name(app: &Arc<App>, said: &str, shuffled: bool, limit: usize, want: Option<&str>) -> Result<super::ByName, ToggleError> {
    let Some(id) = app.playlists.spoken(said).await? else { return Ok(super::ByName { playlist: None, tracks: Vec::new() }) };
    let how = if shuffled { How::Shuffle } else { How::Play };
    let most = limit.clamp(1, super::playlists::PLAY_MAX);
    let tracks = match choose(app, want, &Collection::Playlist { id: id.clone() }, how, None, most, None).await {
        Ok(c) => c.tracks.into_iter().map(|t| t.id).collect(),
        Err(ToggleError::CannotPlay(_)) => Vec::new(),
        Err(e) => return Err(e),
    };
    Ok(super::ByName { playlist: Some(id), tracks })
}

/// A song of Cratarr's own library as players hold it.
pub(super) fn own_track(s: crate::serve::api::Song) -> QueueTrack {
    let named = |s: String| Some(s).filter(|s| !s.is_empty());
    QueueTrack {
        source: source::cratarr::KIND.id.to_string(),
        id: s.id,
        name: s.title,
        artist: named(s.artist),
        album: named(s.album),
        album_id: named(s.album_id),
        duration_secs: Some(s.duration as f64).filter(|d| *d > 0.0),
        ..QueueTrack::default()
    }
}

/// The favourites of `owner`, or all of Cratarr's own library, less what `owner` excluded from automatic picks.
async fn own_candidates(app: &Arc<App>, what: &Collection, owner: Option<&str>) -> Result<Candidates, ToggleError> {
    let Some(served) = app.served.clone() else { return Err(ToggleError::Refused("Cratarr's own library is not open.".into())) };
    let excluded = app.playlists.excluded_songs_of(owner).await?;
    let favourites = matches!(what, Collection::Favourites);
    let owner = owner.map(str::to_string);
    let mut ids = blocking(move || {
        if favourites {
            let owner = owner.ok_or_else(|| anyhow::anyhow!("there is no account yet: make one in Settings"))?;
            served.userdata.starred(&owner, crate::userdata::Kind::Song)
        } else {
            served.library.all_song_ids()
        }
    })
    .await?;
    ids.retain(|id| !excluded.contains(id));
    let name = Some(if favourites { "Favourites" } else { "Library" }.to_string());
    Ok(Candidates { own: true, ..Candidates::of(source::cratarr::KIND.id.to_string(), ids, name) })
}

async fn candidates(app: &Arc<App>, want: Option<&str>, what: &Collection, owner: Option<&str>) -> Result<Candidates, ToggleError> {
    let cat = app.library.catalogue();
    match what {
        Collection::Favourites | Collection::Library => own_candidates(app, what, owner).await,
        Collection::Crate { id, favourites, favourite_albums, without, filter, order, down } => {
            let name = app.playlists.crate_def(id).await?.name;
            let look = super::CrateLook {
                favourites: *favourites,
                favourite_albums: *favourite_albums,
                without: without.clone(),
                filter: filter.clone(),
                order: *order,
                down: *down,
            };
            let shown = app.playlists.crate_shown(id, &look).await?;
            let (source, lines) = app.playlists.crate_lines_in_order(&shown, want, &look).await?;
            let sid = source.clone();
            // A crate's lines are the libraries' files as the catalogue has them: there is no missing one to leave out.
            let (ids, lines): (Vec<String>, Vec<String>) = blocking(move || {
                let found = cat.line_track_ids(&sid, &lines)?;
                Ok(lines.into_iter().zip(found).filter_map(|(l, id)| Some((id?, l))).unzip())
            })
            .await?;
            Ok(Candidates { lines: Some(lines), ..Candidates::of(source, ids, Some(name)) })
        }
        Collection::Playlist { id } => {
            let r = app.playlists.reach(id)?;
            let entries = r.store.entries(&r.name).await?;
            let (source, _) = app.playlists.source_of(want)?;
            let (sid, music) = (source.clone(), app.library.music().map(std::path::PathBuf::from));
            let count = entries.len();
            // As its view plays it: the lines whose file is there and known to the server.
            let (ids, lines): (Vec<String>, Vec<String>) = blocking(move || {
                let found = cat.line_track_ids(&sid, &entries)?;
                Ok(entries
                    .into_iter()
                    .zip(found)
                    .filter(|(l, _)| super::playlists::line_exists(&cat, music.as_deref(), l))
                    .filter_map(|(l, id)| Some((id?, l)))
                    .unzip())
            })
            .await?;
            Ok(Candidates { lines: Some(lines), unknown: count - ids.len(), ..Candidates::of(source, ids, Some(id.clone())) })
        }
        Collection::Album { source, id } => {
            let v = album_view(app, source, id).await?;
            let album = v.id.clone();
            let named: Vec<QueueTrack> = v
                .rows
                .into_iter()
                .filter(|r| !r.missing)
                .filter_map(|r| {
                    Some(QueueTrack {
                        source: v.source.clone(),
                        id: r.id?,
                        name: r.name.unwrap_or(r.file),
                        artist: r.artist,
                        album: r.album,
                        album_id: Some(album.clone()),
                        duration_secs: r.duration_secs,
                        ..QueueTrack::default()
                    })
                })
                .collect();
            let ids = named.iter().map(|t| t.id.clone()).collect();
            Ok(Candidates { named: Some(named), ..Candidates::of(v.source, ids, Some(v.title)) })
        }
        Collection::Artist { source, name } => {
            let v = app.playing.artist_view(source, name).await?;
            let named: Vec<QueueTrack> = v
                .rows
                .into_iter()
                .filter(|r| !r.missing)
                .filter_map(|r| {
                    Some(QueueTrack {
                        source: v.source.clone(),
                        id: r.id?,
                        name: r.name.unwrap_or(r.file),
                        artist: r.artist,
                        album: r.album,
                        album_id: r.album_id,
                        duration_secs: r.duration_secs,
                        ..QueueTrack::default()
                    })
                })
                .collect();
            let ids = named.iter().map(|t| t.id.clone()).collect();
            Ok(Candidates { named: Some(named), ..Candidates::of(v.source, ids, Some(v.name)) })
        }
        Collection::Tracks { source, ids } => {
            let ids: Vec<String> = ids.iter().filter(|i| !i.is_empty()).cloned().collect();
            Ok(Candidates::of(source.clone(), ids, None))
        }
    }
}

/// An album's view, following one that left Incoming to where it is now (as the album panel does).
async fn album_view(app: &Arc<App>, source: &str, id: &str) -> Result<super::AlbumView, ToggleError> {
    let failed = match app.playing.album_view(source, id).await {
        Ok(v) if !v.rows.is_empty() => return Ok(v),
        Ok(v) => Ok(v),
        Err(e) => Err(e),
    };
    match app.incoming.album_fate(source, id).await {
        Some(super::AlbumFate::Moved(now)) if now != id => app.playing.album_view(source, &now).await,
        _ => failed,
    }
}

/// The chosen tracks with their names, and the first one's place among them (a track that cannot be named any more is
/// left out).
async fn tracks_of(app: &Arc<App>, c: &Candidates, chosen: &[usize], index: usize) -> Result<(Vec<QueueTrack>, usize), ToggleError> {
    let numbered = |mut t: QueueTrack, at: usize| {
        t.n = u32::try_from(at).ok();
        t
    };
    let found: Vec<Option<QueueTrack>> = if let Some(named) = &c.named {
        chosen.iter().map(|&i| Some(numbered(named[i].clone(), i))).collect()
    } else if c.own {
        let Some(served) = app.served.clone() else { return Err(ToggleError::Refused("Cratarr's own library is not open.".into())) };
        let ids: Vec<String> = chosen.iter().map(|&i| c.ids[i].clone()).collect();
        let mut songs: std::collections::HashMap<String, crate::serve::api::Song> =
            blocking(move || served.library.songs_by_id(&ids)).await?.into_iter().map(|s| (s.id.clone(), s)).collect();
        chosen.iter().map(|&i| songs.remove(&c.ids[i]).map(|s| numbered(own_track(s), i))).collect()
    } else if let Some(lines) = &c.lines {
        let rows = app.playlists.rows_of(chosen.iter().map(|&i| lines[i].clone()).collect(), &c.source).await?;
        rows.into_iter()
            .zip(chosen)
            .map(|(r, &i)| {
                let id = r.id.filter(|id| *id == c.ids[i])?;
                Some(numbered(
                    QueueTrack {
                        source: c.source.clone(),
                        id,
                        name: r.name.unwrap_or(r.file),
                        artist: r.artist,
                        album: r.album,
                        album_id: r.album_id,
                        duration_secs: r.duration_secs,
                        ..QueueTrack::default()
                    },
                    i,
                ))
            })
            .collect()
    } else {
        // Named by their ids: what the catalogue knows of them (a track outside it plays without; the player asks).
        let (cat, sid, ids) = (app.library.catalogue(), c.source.clone(), chosen.iter().map(|&i| c.ids[i].clone()).collect::<Vec<_>>());
        let rows = blocking(move || cat.source_rows(&sid, &ids)).await.unwrap_or_default();
        chosen
            .iter()
            .enumerate()
            .map(|(k, &i)| {
                let r = rows.get(k);
                Some(numbered(
                    QueueTrack {
                        source: c.source.clone(),
                        id: c.ids[i].clone(),
                        name: r.and_then(|r| r.title.clone()).unwrap_or_default(),
                        artist: r.and_then(|r| r.artist.clone()),
                        album: r.and_then(|r| r.album.clone()),
                        duration_secs: r.and_then(|r| r.duration_ms).map(|ms| ms as f64 / 1000.0),
                        ..QueueTrack::default()
                    },
                    i,
                ))
            })
            .collect()
    };
    let index = found[..index.min(found.len())].iter().filter(|t| t.is_some()).count();
    let tracks: Vec<QueueTrack> = found.into_iter().flatten().collect();
    let index = index.min(tracks.len().saturating_sub(1));
    Ok((tracks, index))
}

/// Queues them on the server's player: Play and Shuffle replace what it plays (from the first to play), Play next and
/// Add to queue go to the player that plays. A Jellyfin player (Finamp) holds fewer than a queue here: it gets what it
/// holds. Answers with the player's label and how many it was sent.
async fn on_server(app: &Arc<App>, source_id: &str, tracks: &[QueueTrack], index: usize, how: How) -> Result<(String, usize), ToggleError> {
    let (active, s) = app
        .library
        .connected()
        .into_iter()
        .find(|(a, _)| a.cfg.id == source_id)
        .ok_or_else(|| ToggleError::CannotPlay("That server is not connected.".into()))?;
    let most = if active.cfg.kind == source::jellyfin::KIND.id { super::playlists::PLAY_MAX } else { QUEUE_MOST };
    let from = if matches!(how, How::Play | How::Shuffle) { index } else { 0 };
    let ids: Vec<String> = tracks[from..].iter().take(most).map(|t| t.id.clone()).collect();
    let label = match how {
        How::Play | How::Shuffle => s.play_tracks(&ids, PlayHow::Now).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}")))?,
        How::Next => app.playing.queue_add(Some(source_id), &ids, PlayHow::Next).await?,
        How::Last => app.playing.queue_add(Some(source_id), &ids, PlayHow::Last).await?,
    };
    Ok((label, ids.len()))
}

/// Queues them on one of Cratarr's players (Play On): a new queue under the collection's name, or added by hand.
async fn on_player(app: &Arc<App>, name: &str, tracks: Vec<QueueTrack>, index: usize, how: How, queue_name: Option<String>) -> Result<(), ToggleError> {
    use super::PlayOnCommand as Command;
    let command = match how {
        How::Play | How::Shuffle => Command::Play {
            from: Default::default(),
            tracks: Some(tracks),
            index,
            position_ms: 0,
            shuffle: false,
            paused: false,
            queue_id: None,
            queue_name,
        },
        How::Next | How::Last => Command::Add {
            from: Default::default(),
            tracks,
            at: None,
            after: None,
            how: Some(if how == How::Next { "next" } else { "last" }.into()),
        },
    };
    app.play_on.command(name, command).await
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> Result<T, ToggleError> {
    Ok(tokio::task::spawn_blocking(f).await.map_err(|e| anyhow::anyhow!("{e}"))??)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_request_names_each_kind_of_collection() {
        let r: Request = serde_json::from_value(serde_json::json!({
            "what": { "kind": "crate", "id": "unlisted", "favourites": "only" }, "how": "shuffle", "on": "server"
        }))
        .unwrap();
        assert_eq!(r.what, Collection::Crate { id: "unlisted".into(), favourites: Some(Favourites::Only), favourite_albums: None, without: Vec::new(), filter: Vec::new(), order: Default::default(), down: false });
        assert_eq!((r.how, r.on), (How::Shuffle, Some(On::Server)));
        let w: Request = serde_json::from_value(serde_json::json!({
            "what": { "kind": "crate", "id": "unlisted", "without": ["cratarr//media/Incoming"] }, "how": "play"
        }))
        .unwrap();
        assert_eq!(w.what, Collection::Crate { id: "unlisted".into(), favourites: None, favourite_albums: None, without: vec!["cratarr//media/Incoming".into()], filter: Vec::new(), order: Default::default(), down: false });
        let r: Request = serde_json::from_value(serde_json::json!({
            "what": { "kind": "tracks", "source": "cratarr", "ids": ["tr-1", "tr-2"] }, "how": "next", "on": { "player": "Lenovo Tab" }
        }))
        .unwrap();
        assert_eq!(r.what, Collection::Tracks { source: "cratarr".into(), ids: vec!["tr-1".into(), "tr-2".into()] });
        assert_eq!(r.on, Some(On::Player("Lenovo Tab".into())));
        for what in [
            serde_json::json!({ "kind": "playlist", "id": "Chill" }),
            serde_json::json!({ "kind": "album", "source": "cratarr", "id": "al-1" }),
            serde_json::json!({ "kind": "artist", "source": "cratarr", "name": "Air" }),
            serde_json::json!({ "kind": "crate", "id": "genre:Jazz" }),
        ] {
            let r: Request = serde_json::from_value(serde_json::json!({ "what": what, "how": "play" })).unwrap();
            assert_eq!((r.on, r.from), (None, None));
        }
    }
}
