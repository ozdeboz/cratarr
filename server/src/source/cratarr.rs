//! Cratarr as its own source (decision 0032): the library it serves over OpenSubsonic (decision 0030) seen through the
//! Source trait, so the pages, the Android app and the playlist and Incoming features work without a media server.
//! Tracks are `tr-<n>` and albums `al-…`, as the OpenSubsonic API hands them out; paths are the files' own, as the
//! container sees them. Favourites, plays and ratings are the owner account's (the first Subsonic account).
//!
//! Its libraries are the folders Cratarr has mounted (as Settings offers music folders, decision 0009): within a mount
//! that holds other mounts, each folder with audio; any other mount with audio, itself (see `library_folders`). A
//! library's id is its folder's path.
//!
//! The player side (what plays, the controls, Play On) is decision 0031's and not here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};

use super::{Album, AlbumDetails, AlbumTrack, Choice, Field, Fut, Kind, Library, LibraryScan, Listed, PlaylistEvent, Source, Status, Track, TrackInfo, TrackPage, Values};
use crate::serve::Serve;
use crate::userdata::{Annotation, Kind as Item};

pub const KIND: Kind = Kind { id: "cratarr", label: "Cratarr", fields, connect };

fn fields() -> Vec<Field> {
    Vec::new()
}

fn connect(_values: &Values, _partial: bool) -> Result<Box<dyn Source>> {
    Ok(Box::new(CratarrSource))
}

/// Audio file extensions a library is recognised by.
const AUDIO: [&str; 12] = ["flac", "mp3", "m4a", "aac", "ogg", "oga", "opus", "wav", "aif", "aiff", "wv", "ape"];

/// How many entries a look for audio reads before it gives up on a folder.
const LOOK_LIMIT: usize = 4000;

fn is_audio(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| AUDIO.iter().any(|a| a.eq_ignore_ascii_case(e)))
}

/// Up to `n` audio files under `dir`, breadth first in name order, reading at most `LOOK_LIMIT` entries. Hidden
/// folders (a trash, a playlist folder) are not looked in.
fn audio_under(dir: &Path, n: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut queue = std::collections::VecDeque::from([dir.to_path_buf()]);
    let mut read = 0;
    while let Some(d) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
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
                Ok(t) if t.is_dir() => queue.push_back(p),
                Ok(t) if t.is_file() && is_audio(&p) => found.push(p),
                _ => {}
            }
        }
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

struct CratarrSource;

fn server() -> Result<Arc<Serve>> {
    crate::serve::instance().context("Cratarr's library is still starting")
}

/// The owner account: whose favourites, plays and ratings these are.
fn owner(s: &Serve) -> Result<String> {
    s.users.owner().context("there is no Subsonic account yet: make one in Settings")
}

/// The catalogue folder a library (by its id, the folder's path) is, as the library was last built.
fn folder_of(s: &Serve, library: &str) -> Option<i64> {
    s.library.roots().into_iter().find(|(_, r)| Path::new(r) == Path::new(library)).map(|(f, _)| f)
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
    serde_json::json!({ "play_count": a.play_count, "play_date": a.play_date, "rating": a.rating, "starred_at": a.starred_at })
}

fn annotation_of(v: &serde_json::Value) -> Annotation {
    Annotation {
        play_count: v["play_count"].as_u64().unwrap_or(0) as u32,
        play_date: v["play_date"].as_i64(),
        rating: v["rating"].as_u64().unwrap_or(0).min(5) as u8,
        starred_at: v["starred_at"].as_i64(),
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
        Box::pin(async move {
            // From the mounts alone: answered also while the server starts (the app asks first thing).
            let exclude: Vec<String> = data_dir().into_iter().collect();
            blocking(move || {
                let exclude: Vec<&str> = exclude.iter().map(String::as_str).collect();
                let mounts = crate::mounts::list(&exclude);
                Ok(library_folders(&mounts)
                    .into_iter()
                    .map(|p| {
                        let id = p.display().to_string();
                        let name = p.file_name().map_or_else(|| id.clone(), |n| n.to_string_lossy().into_owned());
                        Library { id: id.clone(), name, paths: vec![id] }
                    })
                    .collect())
            })
            .await
        })
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

    fn now_playing(&self) -> Fut<'_, Option<super::Playing>> {
        // What plays is the players' (decision 0031).
        Box::pin(async { Ok(None) })
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
                for (a, dir, format, kbps) in lib.albums_in(folder, limit)? {
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
                            tags: ud.tags_of(&base)?,
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
            Ok(facts.map(|(tracks, format, bitrate_kbps)| AlbumDetails { tracks, format, bitrate_kbps }))
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

    /// Plays added up, the later last-played, the better rating, a star when either has one.
    fn combine_user_data(&self, a: &serde_json::Value, b: &serde_json::Value) -> serde_json::Value {
        let (a, b) = (annotation_of(a), annotation_of(b));
        annotation_json(&Annotation {
            play_count: a.play_count + b.play_count,
            play_date: a.play_date.max(b.play_date),
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
            Ok(LibraryScan { running: s.has_catalogue().then(|| s.catalogue().doing()).flatten().map(|_| 0.0), last_end: Some(s.built_at()) })
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
    fn combined_play_data_adds_up() {
        let s = CratarrSource;
        let a = serde_json::json!({ "play_count": 2, "play_date": 100, "rating": 0, "starred_at": null });
        let b = serde_json::json!({ "play_count": 3, "play_date": 50, "rating": 4, "starred_at": 7 });
        let c = annotation_of(&s.combine_user_data(&a, &b));
        assert_eq!((c.play_count, c.play_date, c.rating, c.starred_at), (5, Some(100), 4, Some(7)));
    }
}
