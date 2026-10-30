//! Cratarr as its own source (decision 0032): the library it serves over OpenSubsonic (decision 0030) seen through the
//! Source trait, so the pages, the Android app and the playlist and Incoming features work without a media server.
//! Tracks are `tr-<n>` and albums `al-…`, as the OpenSubsonic API hands them out; paths are the files' own, as the
//! container sees them. Favourites, plays and ratings are the owner account's (the first Subsonic account).
//!
//! Its libraries are the folders chosen in Settings (decision 0044, as the *arr apps' root folders), in their order:
//! `values["folders"]`. A library's id is its folder's path. The music folders found under the mounts (decision 0009:
//! within a mount that holds other mounts, each folder with audio; any other mount with audio, itself; see
//! `library_folders`) are what an install from before had, and what Settings suggests (`found_folders`,
//! `suggested_folders`).
//!
//! The player side (what plays, the controls, Play On) is decision 0031's and not here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};

use super::{Album, AlbumDetails, AlbumTrack, Choice, Field, Fut, Kind, Library, LibraryScan, Listed, PlaylistEvent, Source, Status, Track, TrackInfo, TrackPage, Values};
use crate::app::{PlayOn, PlayOnCommand as Command};
use crate::serve::Serve;
use crate::userdata::{Annotation, Kind as Item};

pub const KIND: Kind = Kind { id: "cratarr", label: "Cratarr", fields, connect };

fn fields() -> Vec<Field> {
    Vec::new()
}

fn connect(values: &Values, _partial: bool) -> Result<Box<dyn Source>> {
    Ok(Box::new(CratarrSource { folders: folders_of(values) }))
}

/// The chosen folders (decision 0044), in their order: `values["folders"]`, a list of paths. None before any are chosen
/// (an install from before has them filled once, at load, from `found_folders`).
fn folders_of(values: &Values) -> Vec<PathBuf> {
    super::list(values, "folders").into_iter().filter(|x| !x.trim().is_empty()).map(PathBuf::from).collect()
}

/// Audio file extensions a library is recognised by.
const AUDIO: [&str; 12] = ["flac", "mp3", "m4a", "aac", "ogg", "oga", "opus", "wav", "aif", "aiff", "wv", "ape"];

/// How many entries a look for audio reads before it gives up on a folder.
const LOOK_LIMIT: usize = 4000;

fn is_audio(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| AUDIO.iter().any(|a| a.eq_ignore_ascii_case(e)))
}

/// Up to `n` audio files under `dir`, depth first in name order (a folder's own files before its folders), reading at
/// most `LOOK_LIMIT` entries. Depth first, a library's first album is a few reads away however many artists it holds:
/// breadth first read every artist's folder before any album (seconds on a cold disk, Settings' placement waiting on
/// it, 2026-10-17), and a large library could spend the whole limit on folders and find nothing. Hidden folders (a
/// trash, a playlist folder) are not looked in.
fn audio_under(dir: &Path, n: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    let mut read = 0;
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        let mut dirs = Vec::new();
        for e in entries {
            read += 1;
            if read > LOOK_LIMIT || found.len() >= n {
                return found;
            }
            let name = e.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            let p = e.path();
            match e.file_type() {
                Ok(t) if t.is_dir() => dirs.push(p),
                Ok(t) if t.is_file() && is_audio(&p) => found.push(p),
                _ => {}
            }
        }
        if found.len() >= n {
            return found;
        }
        // Its folders next, the first by name first.
        stack.extend(dirs.into_iter().rev());
    }
    found
}

/// The music folders among the mounts (see the module's notes): a mount with other mounts inside it (the NAS's
/// `/media`, holding `/media/Music`) is a folder of libraries, each folder directly under it with audio being one; any
/// other mount with audio (`/music`, holding artists) is one library itself.
pub fn library_folders(mounts: &[String]) -> Vec<PathBuf> {
    let mounts: Vec<PathBuf> = mounts.iter().map(PathBuf::from).collect();
    let inside = |m: &PathBuf, o: &PathBuf| o != m && o.starts_with(m);
    let tops: Vec<&PathBuf> = mounts.iter().filter(|m| !mounts.iter().any(|o| inside(o, m))).collect();
    let mut out = Vec::new();
    for top in tops {
        if mounts.iter().any(|o| inside(top, o)) {
            let mut subs: Vec<PathBuf> = std::fs::read_dir(top)
                .map(|r| r.filter_map(|e| e.ok()).filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()).collect())
                .unwrap_or_default();
            subs.retain(|p| !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')));
            subs.sort();
            out.extend(subs.into_iter().filter(|s| !audio_under(s, 1).is_empty()));
        } else if !audio_under(top, 1).is_empty() {
            out.push(top.clone());
        }
    }
    out
}

/// The music folders found under Cratarr's mounts (its own data folder left out): the libraries before folders were
/// chosen (decision 0044), so an install from before keeps them. Reads the disk: call it off the async threads.
pub fn found_folders() -> Vec<PathBuf> {
    let exclude: Vec<String> = data_dir().into_iter().collect();
    let exclude: Vec<&str> = exclude.iter().map(String::as_str).collect();
    library_folders(&crate::mounts::list(&exclude))
}

/// The found music folders Settings offers to add: those not chosen yet, nor inside a chosen one, nor holding one (two
/// libraries over the same files). Reads the disk: call it off the async threads.
pub fn suggested_folders(chosen: &[String]) -> Vec<PathBuf> {
    suggest(found_folders(), chosen)
}

fn suggest(found: Vec<PathBuf>, chosen: &[String]) -> Vec<PathBuf> {
    let chosen: Vec<PathBuf> = chosen.iter().map(PathBuf::from).collect();
    found.into_iter().filter(|f| !chosen.iter().any(|c| f.starts_with(c) || c.starts_with(f))).collect()
}

struct CratarrSource {
    /// The chosen folders, in their order (decision 0044).
    folders: Vec<PathBuf>,
}

/// The player Cratarr follows, by name (any online one when none has anything loaded).
/// Player `name` where it is known, else the followed one (0.9.8: carry-on resumes on the player it stopped).
fn player_or_followed(name: Option<&str>) -> Result<(Arc<PlayOn>, String)> {
    if let Some(n) = name
        && let Some(on) = PlayOn::instance()
        && on.get(n).is_some()
    {
        return Ok((on, n.to_string()));
    }
    followed()
}

fn followed() -> Result<(Arc<PlayOn>, String)> {
    let on = PlayOn::instance().context("Play On is not running")?;
    let name = on.followed().map(|p| p.name).context("no Cratarr player is connected")?;
    Ok((on, name))
}

/// Tracks of this source by id, as players take them (their names are filled in by Play On).
fn queue_tracks(ids: &[String]) -> Vec<crate::app::QueueTrack> {
    ids.iter().map(|id| crate::app::QueueTrack { source: KIND.id.to_string(), id: id.clone(), ..Default::default() }).collect()
}

/// This source's id (`tr-…`) of a player's track, whichever source it was queued from. Blocking.
fn own_id(s: &Serve, source: &str, id: &str) -> Option<String> {
    if source == KIND.id {
        return Some(id.to_string());
    }
    s.catalogue().track_of_source(source, id).map(crate::serve::ids::song)
}

fn server() -> Result<Arc<Serve>> {
    crate::serve::instance().context("Cratarr's library is still starting")
}

/// The account whose favourites, plays and ratings these are: the request's (decision 0051).
fn owner(s: &Serve) -> Result<String> {
    s.current_user().context("no one is signed in, or there is no account yet: make one in Settings")
}

/// The catalogue folder a library (by its id, the folder's path) is, as the library was last built.
fn folder_of(s: &Serve, library: &str) -> Option<i64> {
    s.library.roots().into_iter().find(|(_, r)| Path::new(r) == Path::new(library)).map(|(f, _)| f)
}

/// One of the library's albums as an album card, without what only a listing reads (its folder, format, bitrate,
/// length and tags): an artist's albums, Home's New for you before the lists were read again.
pub fn card(a: crate::serve::api::Album) -> Album {
    Album {
        added: a.created.as_deref().and_then(crate::nowplaying::parse_iso_epoch),
        title: a.name,
        artist: Some(a.artist),
        year: a.year,
        path: None,
        tracks: a.song_count,
        format: None,
        bitrate_kbps: None,
        duration_secs: None,
        tags: Vec::new(),
        library: None,
        artists: if a.artists.len() > 1 { a.artists.iter().map(|r| r.name.clone()).collect() } else { Vec::new() },
        release: a.release,
        id: a.id,
    }
}

/// A catalogue folder's path, as the library was last built.
fn root_of(s: &Serve, folder: i64) -> Option<String> {
    s.library.roots().into_iter().find(|(f, _)| *f == folder).map(|(_, r)| r)
}

/// Cratarr's data folder, left out of the libraries: the server's, else `CRATARR_DATA_DIR` while it starts.
fn data_dir() -> Option<String> {
    match crate::serve::instance() {
        Some(s) => s.data_dir.as_ref().map(|d| d.display().to_string()),
        None => std::env::var("CRATARR_DATA_DIR").ok().filter(|d| !d.trim().is_empty()),
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f).await?
}

fn annotation_json(a: &Annotation) -> serde_json::Value {
    serde_json::json!({
        "play_count": a.play_count, "play_date": a.play_date, "rating": a.rating, "starred_at": a.starred_at,
        "own_count": a.own_count, "own_date": a.own_date,
    })
}

/// Kept from before one's own plays were apart (2026-10-30): all of them one's own.
fn annotation_of(v: &serde_json::Value) -> Annotation {
    let play_count = v["play_count"].as_u64().unwrap_or(0) as u32;
    let play_date = v["play_date"].as_i64();
    let own = v.get("own_count").is_some();
    Annotation {
        play_count,
        play_date,
        rating: v["rating"].as_u64().unwrap_or(0).min(5) as u8,
        starred_at: v["starred_at"].as_i64(),
        own_count: if own { (v["own_count"].as_u64().unwrap_or(0) as u32).min(play_count) } else { play_count },
        own_date: if own { v["own_date"].as_i64() } else { play_date },
    }
}

impl Source for CratarrSource {
    fn status(&self) -> Fut<'_, Status> {
        Box::pin(async move {
            Ok(match crate::serve::instance() {
                None => Status { error: Some("Cratarr's library is still starting".into()), ..Status::default() },
                Some(s) => Status {
                    connected: true,
                    server_name: Some("Cratarr".into()),
                    version: Some(env!("CARGO_PKG_VERSION").into()),
                    detail: s.users.owner().map(|o| format!("as {o}")),
                    error: None,
                },
            })
        })
    }

    fn options(&self) -> Fut<'_, BTreeMap<String, Vec<Choice>>> {
        Box::pin(async { Ok(BTreeMap::new()) })
    }

    fn libraries(&self) -> Fut<'_, Vec<Library>> {
        // The chosen folders, as they are: answered also while the server starts (the app asks first thing). A chosen
        // folder that is missing now (unmounted) is still listed, so its use and policy stay; Settings shows it missing.
        let libraries = self
            .folders
            .iter()
            .map(|p| {
                let id = p.display().to_string();
                let name = p.file_name().map_or_else(|| id.clone(), |n| n.to_string_lossy().into_owned());
                Library { id: id.clone(), name, paths: vec![id] }
            })
            .collect();
        Box::pin(async move { Ok(libraries) })
    }

    fn sample_paths<'a>(&'a self, library_id: &'a str, n: usize) -> Fut<'a, Vec<String>> {
        Box::pin(async move {
            let dir = PathBuf::from(library_id);
            blocking(move || Ok(audio_under(&dir, n).into_iter().map(|p| p.display().to_string()).collect())).await
        })
    }

    fn library_roots<'a>(&'a self, library_id: &'a str) -> Fut<'a, Vec<String>> {
        Box::pin(async move { Ok(vec![library_id.to_string()]) })
    }

    // --- What plays and the commands: Cratarr's own players (Play On, decision 0031), the one it follows. ---

    fn now_playing(&self) -> Fut<'_, Option<super::Playing>> {
        self.now_playing_on(None)
    }

    fn now_playing_on<'a>(&'a self, player: Option<&'a str>) -> Fut<'a, Option<super::Playing>> {
        Box::pin(async move {
            let Some(on) = PlayOn::instance() else { return Ok(None) };
            let Some(p) = (match player {
                Some(n) => on.get(n),
                None => on.followed(),
            }) else {
                return Ok(None);
            };
            let queue = p.queue.clone().unwrap_or_default();
            let Ok(at) = usize::try_from(p.index) else { return Ok(None) };
            let Some(here) = queue.get(at) else { return Ok(None) };
            let s = server()?;
            let ids: Vec<(String, String)> = queue[at..].iter().take(21).map(|t| (t.source.clone(), t.id.clone())).collect();
            // Described by Cratarr (0.9.6, decision 0033 amended): what the track is, its album, its file and the
            // account's favourite, whatever the player reported.
            let account = owner(&s).ok();
            let ours = blocking(move || crate::app::describe::ours(&s, &ids, account.as_deref())).await?;
            let track_of = |t: &crate::app::QueueTrack, d: &Option<crate::app::describe::Described>| match d {
                Some(d) => Track {
                    id: d.own.clone().unwrap_or_else(|| t.id.clone()),
                    title: d.name.clone(),
                    artist: d.artist.clone(),
                    album: d.album.clone(),
                    album_id: d.album_id.clone(),
                    path: d.path.clone(),
                    library_id: None,
                },
                None => Track { id: t.id.clone(), title: t.name.clone(), artist: t.artist.clone(), album: t.album.clone(), album_id: t.album_id.clone(), path: None, library_id: None },
            };
            Ok(Some(super::Playing {
                track: track_of(here, &ours[0]),
                progress: Some(crate::nowplaying::Progress {
                    position_secs: p.position_ms as f64 / 1000.0,
                    duration_secs: p.duration_ms.map(|d| d as f64 / 1000.0),
                    paused: !p.playing,
                    estimated: false,
                    rewinding: p.rewinding,
                    starting: p.starting,
                }),
                player: Some(p.name.clone()),
                started_at: None,
                favorite: ours[0].as_ref().and_then(|d| d.favorite),
                upcoming: queue[at + 1..]
                    .iter()
                    .zip(ours.iter().skip(1))
                    .map(|(t, d)| super::Upcoming {
                        track: track_of(t, d),
                        duration_secs: d.as_ref().and_then(|d| d.duration_secs).or(t.duration_secs),
                        asked_by: t.asked_by.clone(),
                    })
                    .collect(),
            }))
        })
    }

    fn players(&self) -> Fut<'_, Option<String>> {
        Box::pin(async { Ok(PlayOn::instance().map(|on| on.summary())) })
    }

    fn transport<'a>(&'a self, command: super::Transport) -> Fut<'a, String> {
        self.transport_on(command, None)
    }

    fn transport_on<'a>(&'a self, command: super::Transport, player: Option<&'a str>) -> Fut<'a, String> {
        Box::pin(async move {
            let (on, name) = player_or_followed(player)?;
            let c = match command {
                super::Transport::Previous => Command::Previous,
                super::Transport::PlayPause => Command::Toggle,
                super::Transport::Next => Command::Next,
                super::Transport::Stop => Command::Stop { moved_to: None },
            };
            on.command(&name, c).await.map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(name)
        })
    }

    fn play_tracks<'a>(&'a self, ids: &'a [String], how: super::PlayHow) -> Fut<'a, String> {
        Box::pin(async move {
            let (on, name) = followed()?;
            let tracks = queue_tracks(ids);
            // Next and Last are added by hand: Play On places them (decision 0033's lanes).
            let c = match how {
                super::PlayHow::Now => Command::play(tracks, 0, 0),
                super::PlayHow::Next => Command::Add { from: Default::default(), tracks, at: None, after: None, how: Some("next".into()) },
                super::PlayHow::Last => Command::Add { from: Default::default(), tracks, at: None, after: None, how: Some("last".into()) },
            };
            on.command(&name, c).await.map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(name)
        })
    }

    fn play_from<'a>(&'a self, ids: &'a [String], start_secs: f64) -> Fut<'a, String> {
        self.play_from_on(ids, start_secs, None)
    }

    fn play_from_on<'a>(&'a self, ids: &'a [String], start_secs: f64, player: Option<&'a str>) -> Fut<'a, String> {
        Box::pin(async move {
            let (on, name) = player_or_followed(player)?;
            let c = Command::play(queue_tracks(ids), 0, (start_secs * 1000.0) as u64);
            on.command(&name, c).await.map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(name)
        })
    }

    fn jump_to<'a>(&'a self, track: &'a str) -> Fut<'a, String> {
        Box::pin(async move {
            let (on, name) = followed()?;
            let p = on.followed().context("no Cratarr player plays")?;
            let queue = p.queue.unwrap_or_default();
            let s = server()?;
            let ids: Vec<(String, String)> = queue.iter().map(|t| (t.source.clone(), t.id.clone())).collect();
            let want = track.to_string();
            let found = blocking(move || {
                Ok(ids.iter().enumerate().skip(usize::try_from(p.index).unwrap_or(0)).find(|(_, (src, id))| own_id(&s, src, id).as_deref() == Some(want.as_str())).map(|(i, _)| i))
            })
            .await?;
            let index = found.context("that track is not in the player's queue")?;
            on.command(&name, Command::Jump { index: Some(index), entry: None }).await.map_err(|e| anyhow::anyhow!("{e}"))?;
            Ok(name)
        })
    }

    /// A play counts for the owner's account, as a Subsonic app's scrobble does (decision 0030).
    fn count_play<'a>(&'a self, track: &'a str) -> Fut<'a, ()> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        self.count_play_at(track, now)
    }

    /// A play told late keeps its own time (decision 0033).
    fn count_play_at<'a>(&'a self, track: &'a str, at: i64) -> Fut<'a, ()> {
        Box::pin(async move {
            let s = server()?;
            let me = owner(&s)?;
            // One way in for listening (decision 0068).
            crate::app::listening::heard(&s, &me, track, &crate::userdata::Heard::counted(at)).await?;
            Ok(())
        })
    }

    fn track<'a>(&'a self, id: &'a str) -> Fut<'a, Track> {
        Box::pin(async move {
            let s = server()?;
            let (lib, want) = (s.library.clone(), id.to_string());
            let found = blocking(move || {
                let song = lib.songs_by_id(std::slice::from_ref(&want))?.pop();
                let place = lib.file_of(&want)?;
                Ok(song.zip(place))
            })
            .await?;
            let Some((song, (folder, _))) = found else { bail!("no such track") };
            let library_id = root_of(&s, folder);
            Ok(Track {
                id: song.id,
                title: song.title,
                artist: Some(song.display_album_artist).filter(|a| !a.is_empty()).or(Some(song.artist)),
                album: Some(song.album),
                album_id: Some(song.album_id),
                path: Some(song.path),
                library_id,
            })
        })
    }

    /// The playlist files are the truth here: nothing to tell.
    fn playlists_changed(&self, _events: Vec<PlaylistEvent>) -> Fut<'_, ()> {
        Box::pin(async { Ok(()) })
    }

    fn lists_albums(&self) -> bool {
        true
    }

    fn albums<'a>(&'a self, library_id: &'a str, limit: usize) -> Fut<'a, Vec<Listed>> {
        Box::pin(async move {
            let s = server()?;
            let Some(folder) = folder_of(&s, library_id) else { return Ok(Vec::new()) };
            let (lib, ud) = (s.library.clone(), s.userdata.clone());
            let root = library_id.trim_end_matches('/').to_string();
            blocking(move || {
                let mut out = Vec::new();
                for (a, dir, format, kbps, secs) in lib.albums_in(folder, limit)? {
                    let base = lib.album_base(&a.id)?.unwrap_or_else(|| a.id.clone());
                    let stamp = format!("{} {} {}", a.song_count, a.created.as_deref().unwrap_or(""), dir);
                    out.push(Listed {
                        album: Album {
                            added: a.created.as_deref().and_then(crate::nowplaying::parse_iso_epoch),
                            title: a.name,
                            artist: Some(a.artist),
                            year: a.year,
                            path: Some(format!("{root}/{dir}")),
                            tracks: a.song_count,
                            format,
                            bitrate_kbps: kbps,
                            duration_secs: secs,
                            tags: ud.tags_of(&base)?,
                            library: None,
                            artists: if a.artists.len() > 1 { a.artists.iter().map(|r| r.name.clone()).collect() } else { Vec::new() },
                            release: a.release,
                            id: a.id,
                        },
                        stamp: Some(stamp),
                    });
                }
                Ok(out)
            })
            .await
        })
    }

    fn album_summary<'a>(&'a self, album: &'a str) -> Fut<'a, Option<AlbumDetails>> {
        Box::pin(async move {
            let s = server()?;
            let (lib, id) = (s.library.clone(), album.to_string());
            let facts = blocking(move || lib.album_facts(&id)).await?;
            Ok(facts.map(|(tracks, format, bitrate_kbps, duration_secs)| AlbumDetails { tracks, format, bitrate_kbps, duration_secs }))
        })
    }

    // Added: the library's (the earlier of the files' first sight and an import's date); played: the owner's plays of
    // it, as Subsonic apps count them.
    fn album_listening<'a>(&'a self, album: &'a str) -> Fut<'a, super::AlbumListening> {
        Box::pin(async move {
            let s = server()?;
            let o = owner(&s)?;
            let (lib, id) = (s.library.clone(), album.to_string());
            let found = blocking(move || lib.albums_by_id(&[id])).await?;
            let added = found.first().and_then(|a| a.created.as_deref()).and_then(crate::nowplaying::parse_iso_epoch);
            let seen = s.userdata.get(&o, Item::Album, &[album.to_string()])?;
            let a = seen.get(album);
            Ok(super::AlbumListening { added, played: a.and_then(|a| a.play_date), plays: Some(a.map_or(0, |a| a.play_count)) })
        })
    }

    // An artist is their name's id (as Subsonic apps see them): the albums crediting it, the songs crediting it. Plays
    // and the heart are the owner's.
    fn artist_works<'a>(&'a self, name: &'a str) -> Fut<'a, Option<super::ArtistWorks>> {
        Box::pin(async move {
            let s = server()?;
            let o = owner(&s)?;
            let id = crate::serve::ids::artist(name);
            let (lib, aid) = (s.library.clone(), id.clone());
            let (own, on, songs) = blocking(move || lib.artist_works(&aid)).await?;
            let ids: Vec<String> = songs.iter().map(|t| t.id.clone()).collect();
            let seen = s.userdata.get(&o, Item::Song, &ids)?;
            let favorite = s.userdata.get(&o, Item::Artist, std::slice::from_ref(&id))?.get(&id).is_some_and(|a| a.starred_at.is_some());
            let tracks = songs
                .into_iter()
                .map(|t| {
                    let a = seen.get(&t.id);
                    super::ArtistTrack {
                        plays: a.map_or(0, |a| a.play_count),
                        played: a.and_then(|a| a.play_date),
                        album_id: t.album_id.clone(),
                        track: AlbumTrack {
                            id: t.id,
                            title: t.title,
                            artist: Some(t.artist).filter(|a| !a.is_empty()),
                            album: Some(t.album),
                            album_artist: Some(t.display_album_artist).filter(|a| !a.is_empty()),
                            year: t.year,
                            path: Some(t.path),
                            duration_secs: Some(t.duration as f64),
                        },
                    }
                })
                .collect();
            Ok(Some(super::ArtistWorks {
                albums: own.into_iter().map(card).collect(),
                appears_on: on.into_iter().map(card).collect(),
                tracks,
                favorite: Some(favorite),
            }))
        })
    }

    fn set_artist_favorite<'a>(&'a self, name: &'a str, on: bool) -> Fut<'a, bool> {
        Box::pin(async move {
            let s = server()?;
            let o = owner(&s)?;
            s.userdata.set_starred(&o, Item::Artist, &[crate::serve::ids::artist(name)], on)?;
            Ok(on)
        })
    }

    fn artist_favorite<'a>(&'a self, name: &'a str) -> Fut<'a, Option<bool>> {
        Box::pin(async move {
            let s = server()?;
            let o = owner(&s)?;
            // As artist_works reads it: the same name, the same id.
            let id = crate::serve::ids::artist(name);
            Ok(Some(s.userdata.get(&o, Item::Artist, std::slice::from_ref(&id))?.get(&id).is_some_and(|a| a.starred_at.is_some())))
        })
    }

    fn album_uses(&self) -> Fut<'_, BTreeMap<String, super::AlbumUse>> {
        Box::pin(async move {
            let s = server()?;
            let o = owner(&s)?;
            Ok(s.userdata
                .used(&o, Item::Album)?
                .into_iter()
                .map(|(id, a)| (id, super::AlbumUse { plays: a.play_count, played: a.play_date, rating: a.rating }))
                .collect())
        })
    }

    fn tracks_of_album<'a>(&'a self, album_id: &'a str) -> Fut<'a, Vec<AlbumTrack>> {
        Box::pin(async move {
            let s = server()?;
            let (lib, id) = (s.library.clone(), album_id.to_string());
            let album = blocking(move || lib.album(&id)).await?.context("no such album")?;
            Ok(album
                .song
                .unwrap_or_default()
                .into_iter()
                .map(|t| AlbumTrack {
                    id: t.id,
                    title: t.title,
                    artist: Some(t.artist).filter(|a| !a.is_empty()),
                    album: Some(t.album),
                    album_artist: Some(t.display_album_artist).filter(|a| !a.is_empty()),
                    year: t.year,
                    path: Some(t.path),
                    duration_secs: Some(t.duration as f64),
                })
                .collect())
        })
    }

    /// One read of the library for them all.
    fn tracks_of_albums<'a>(&'a self, albums: &'a [String]) -> Fut<'a, Vec<(String, AlbumTrack)>> {
        Box::pin(async move {
            let s = server()?;
            let (lib, ids) = (s.library.clone(), albums.to_vec());
            let songs = blocking(move || lib.songs_of_albums(&ids)).await?;
            Ok(songs
                .into_iter()
                .map(|t| {
                    (t.album_id.clone(), AlbumTrack {
                        id: t.id,
                        title: t.title,
                        artist: Some(t.artist).filter(|a| !a.is_empty()),
                        album: Some(t.album),
                        album_artist: Some(t.display_album_artist).filter(|a| !a.is_empty()),
                        year: t.year,
                        path: Some(t.path),
                        duration_secs: Some(t.duration as f64),
                    })
                })
                .collect())
        })
    }

    fn lists_tracks(&self) -> bool {
        true
    }

    /// A library's songs with their paths. `since`: nothing when the library was not built again since then.
    fn track_paths<'a>(&'a self, library_id: &'a str, since: Option<i64>, start: usize, limit: usize) -> Fut<'a, TrackPage> {
        Box::pin(async move {
            let s = server()?;
            if since.is_some_and(|t| s.built_at() < t) {
                return Ok(TrackPage::default());
            }
            let Some(folder) = folder_of(&s, library_id) else { return Ok(TrackPage::default()) };
            let lib = s.library.clone();
            let items = blocking(move || lib.song_paths(folder, start, limit)).await?;
            let total = if items.len() < limit { start + items.len() } else { usize::MAX };
            Ok(TrackPage { items, total })
        })
    }

    fn users(&self) -> Fut<'_, Vec<(String, String)>> {
        Box::pin(async move { Ok(server()?.users.names().into_iter().map(|n| (n.clone(), n)).collect()) })
    }

    fn track_info<'a>(&'a self, track: &'a str) -> Fut<'a, Option<TrackInfo>> {
        Box::pin(async move {
            let s = server()?;
            let (lib, id) = (s.library.clone(), track.to_string());
            let Some(song) = blocking(move || Ok(lib.songs_by_id(std::slice::from_ref(&id))?.pop())).await? else { return Ok(None) };
            let plays = match owner(&s) {
                Ok(o) => s.userdata.get(&o, Item::Song, std::slice::from_ref(&song.id))?.get(&song.id).map(|a| a.play_count),
                Err(_) => None,
            };
            Ok(Some(TrackInfo {
                play_count: plays.or(Some(0)),
                codec: Some(song.suffix.to_uppercase()).filter(|c| !c.is_empty()),
                bitrate_kbps: song.bit_rate,
                bpm: song.bpm,
                ..TrackInfo::default()
            }))
        })
    }

    fn favorites<'a>(&'a self, tracks: &'a [String]) -> Fut<'a, BTreeMap<String, bool>> {
        Box::pin(async move {
            let s = server()?;
            let o = owner(&s)?;
            let found = s.userdata.get(&o, Item::Song, tracks)?;
            Ok(tracks.iter().map(|t| (t.clone(), found.get(t).is_some_and(|a| a.starred_at.is_some()))).collect())
        })
    }

    fn favorite_tracks(&self) -> Fut<'_, Vec<String>> {
        Box::pin(async move {
            let s = server()?;
            let o = owner(&s)?;
            s.userdata.starred(&o, Item::Song)
        })
    }

    fn favorite<'a>(&'a self, track: &'a str) -> Fut<'a, Option<bool>> {
        Box::pin(async move { Ok(self.favorites(&[track.to_string()]).await?.get(track).copied()) })
    }

    // An album's star is the one a Subsonic app gives it (`star?albumId=`), so the panel's heart and the apps' agree.
    fn favorite_albums(&self) -> Fut<'_, Vec<String>> {
        Box::pin(async move {
            let s = server()?;
            let o = owner(&s)?;
            s.userdata.starred(&o, Item::Album)
        })
    }

    // An artist's star is the one a Subsonic app gives it (`star?artistId=`), its id made from its name: the names of
    // those the library has.
    fn favorite_artists(&self) -> Fut<'_, Vec<String>> {
        Box::pin(async move {
            let s = server()?;
            let o = owner(&s)?;
            let ids = s.userdata.starred(&o, Item::Artist)?;
            tokio::task::spawn_blocking(move || Ok(ids.iter().filter_map(|id| s.library.artist(id).ok().flatten()).map(|a| a.name).collect())).await?
        })
    }

    // An album's rating is the one a Subsonic app gives it (`setRating`).
    fn album_rating<'a>(&'a self, album: &'a str) -> Fut<'a, Option<u8>> {
        Box::pin(async move {
            let s = server()?;
            let o = owner(&s)?;
            Ok(Some(s.userdata.get(&o, Item::Album, &[album.to_string()])?.get(album).map_or(0, |a| a.rating)))
        })
    }

    fn set_album_rating<'a>(&'a self, album: &'a str, stars: u8) -> Fut<'a, ()> {
        Box::pin(async move {
            let s = server()?;
            let o = owner(&s)?;
            s.userdata.set_rating(&o, Item::Album, album, stars.min(5))
        })
    }

    fn set_album_favorite<'a>(&'a self, album: &'a str, on: bool) -> Fut<'a, bool> {
        Box::pin(async move {
            let s = server()?;
            let o = owner(&s)?;
            s.userdata.set_starred(&o, Item::Album, &[album.to_string()], on)?;
            Ok(on)
        })
    }

    fn set_favorite<'a>(&'a self, track: &'a str, on: bool) -> Fut<'a, bool> {
        Box::pin(async move {
            let s = server()?;
            let o = owner(&s)?;
            s.userdata.set_starred(&o, Item::Song, &[track.to_string()], on)?;
            Ok(on)
        })
    }

    fn playlist_favorites(&self) -> Fut<'_, BTreeMap<String, bool>> {
        Box::pin(async move {
            let s = server()?;
            let o = owner(&s)?;
            Ok(s.userdata.starred(&o, Item::Playlist)?.into_iter().map(|n| (n, true)).collect())
        })
    }

    fn set_playlist_favorite<'a>(&'a self, name: &'a str, on: bool) -> Fut<'a, ()> {
        Box::pin(async move {
            let s = server()?;
            let o = owner(&s)?;
            s.userdata.set_starred(&o, Item::Playlist, &[name.to_string()], on)
        })
    }

    /// Each account's plays, last played, rating and star on a song, for those with any (decision 0013). Cratarr's
    /// track ids survive a move, so a Keep needs none of this; a Replace (old tracks for new ones) does.
    fn user_data<'a>(&'a self, item: &'a str) -> Fut<'a, Vec<(String, serde_json::Value)>> {
        Box::pin(async move {
            let s = server()?;
            let mut out = Vec::new();
            for user in s.users.names() {
                if let Some(a) = s.userdata.get(&user, Item::Song, &[item.to_string()])?.remove(item)
                    && (a.play_count > 0 || a.starred_at.is_some() || a.rating > 0)
                {
                    out.push((user, annotation_json(&a)));
                }
            }
            Ok(out)
        })
    }

    /// Plays added up (all and one's own), the later last-played, the better rating, a star when either has one.
    fn combine_user_data(&self, a: &serde_json::Value, b: &serde_json::Value) -> serde_json::Value {
        let (a, b) = (annotation_of(a), annotation_of(b));
        annotation_json(&Annotation {
            play_count: a.play_count + b.play_count,
            play_date: a.play_date.max(b.play_date),
            own_count: a.own_count + b.own_count,
            own_date: a.own_date.max(b.own_date),
            rating: a.rating.max(b.rating),
            starred_at: match (a.starred_at, b.starred_at) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (x, y) => x.or(y),
            },
        })
    }

    fn set_user_data<'a>(&'a self, item: &'a str, user: &'a str, data: &'a serde_json::Value) -> Fut<'a, ()> {
        Box::pin(async move {
            let s = server()?;
            let pid = s.library.play_targets(item)?.map(|(pid, _, _)| pid);
            s.userdata.set_annotation(user, Item::Song, item, pid.as_deref(), &annotation_of(data))
        })
    }

    fn library_scan(&self) -> Fut<'_, LibraryScan> {
        Box::pin(async move {
            let s = server()?;
            // While a pass runs, how much it has done (0.9.7): a long one moves, a stuck one does not.
            let cat = s.has_catalogue().then(|| s.catalogue());
            Ok(LibraryScan { running: cat.and_then(|c| c.doing().map(|_| c.worked() as f64)), last_end: Some(s.built_at()) })
        })
    }

    /// Files moved or appeared: the catalogue looks now, and the library follows it.
    fn files_changed<'a>(&'a self, changes: &'a [(String, bool)]) -> Fut<'a, ()> {
        Box::pin(async move {
            let s = server()?;
            if !changes.is_empty() && s.has_catalogue() {
                s.catalogue().poke();
            }
            Ok(())
        })
    }

    fn album_cover<'a>(&'a self, album_id: &'a str) -> Fut<'a, Option<(String, Vec<u8>)>> {
        Box::pin(async move { server()?.cover_bytes(album_id).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libraries_are_the_music_folders_under_the_top_mounts() {
        let root = std::env::temp_dir().join(format!("cratarr-libs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (p, f) in [("media/Music/A/X", "01.flac"), ("media/lidarr-music/B/Y", "01.mp3"), ("media/Films/Z", "movie.mkv"), ("music/C", "01.ogg")] {
            std::fs::create_dir_all(root.join(p)).unwrap();
            std::fs::write(root.join(p).join(f), b"x").unwrap();
        }
        std::fs::create_dir_all(root.join("media/.trash/D")).unwrap();
        std::fs::write(root.join("media/.trash/D/01.flac"), b"x").unwrap();
        let mounts = [root.join("media"), root.join("media/Music"), root.join("music")].map(|p| p.display().to_string());
        let found = library_folders(&mounts);
        assert_eq!(found, [root.join("media/Music"), root.join("media/lidarr-music"), root.join("music")], "films and hidden folders are not music");
        assert_eq!(audio_under(&root.join("media"), 5).len(), 2);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn samples_are_found_depth_first_a_folders_own_files_first() {
        let root = std::env::temp_dir().join(format!("cratarr-dfs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // Many artists before any album breadth first; depth first the first album is three reads away.
        for i in 0..50 {
            std::fs::create_dir_all(root.join(format!("Artist {i:02}/Album"))).unwrap();
        }
        for (p, f) in [("Artist 00/Album", "01.flac"), ("Artist 00/Album", "02.flac"), ("Artist 01", "loose.mp3"), ("", "top.ogg")] {
            std::fs::write(root.join(p).join(f), b"x").unwrap();
        }
        let names = |v: Vec<PathBuf>| v.iter().map(|p| p.strip_prefix(&root).unwrap().display().to_string().replace('\\', "/")).collect::<Vec<_>>();
        assert_eq!(names(audio_under(&root, 3)), ["top.ogg", "Artist 00/Album/01.flac", "Artist 00/Album/02.flac"]);
        assert_eq!(names(audio_under(&root, 4)).last().unwrap(), "Artist 01/loose.mp3");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn libraries_are_the_chosen_folders_in_their_order() {
        let values: Values = [("folders".to_string(), serde_json::json!(["/media/Music", "/media/Incoming", "/gone/Old", ""]))].into();
        let s = connect(&values, false).unwrap();
        let libs = s.libraries().await.unwrap();
        let ids: Vec<&str> = libs.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(ids, ["/media/Music", "/media/Incoming", "/gone/Old"], "a missing folder stays; an empty entry is no folder");
        assert_eq!((libs[1].name.as_str(), libs[1].paths.as_slice()), ("Incoming", ["/media/Incoming".to_string()].as_slice()));
        let none = connect(&Values::new(), false).unwrap();
        assert!(none.libraries().await.unwrap().is_empty(), "none chosen yet: no libraries (the load fills them once)");
    }

    #[test]
    fn suggestions_leave_out_the_chosen_and_what_overlaps_them() {
        let found = ["/media/Music", "/media/Incoming", "/media/lidarr-music", "/music"].map(PathBuf::from).to_vec();
        let chosen = ["/media/Music".to_string(), "/media/lidarr-music/Jazz".to_string(), "/srv".to_string()];
        assert_eq!(suggest(found, &chosen), [PathBuf::from("/media/Incoming"), PathBuf::from("/music")]);
    }

    #[test]
    fn combined_play_data_adds_up() {
        let s = CratarrSource { folders: Vec::new() };
        let a = serde_json::json!({ "play_count": 2, "play_date": 100, "rating": 0, "starred_at": null });
        let b = serde_json::json!({ "play_count": 3, "play_date": 50, "rating": 4, "starred_at": 7 });
        let c = annotation_of(&s.combine_user_data(&a, &b));
        assert_eq!((c.play_count, c.play_date, c.rating, c.starred_at), (5, Some(100), 4, Some(7)));
        assert_eq!((c.own_count, c.own_date), (5, Some(100)), "kept from before: all one's own");
        let followed = serde_json::json!({ "play_count": 3, "play_date": 90, "own_count": 1, "own_date": 40, "rating": 0, "starred_at": null });
        let c = annotation_of(&s.combine_user_data(&a, &followed));
        assert_eq!((c.play_count, c.play_date, c.own_count, c.own_date), (5, Some(100), 3, Some(100)), "one's own added up apart");
    }
}
