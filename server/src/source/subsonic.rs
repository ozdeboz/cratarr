//! A Subsonic server as a source (Navidrome, and any server speaking the Subsonic API with real paths). One account
//! logs in; its favourites, plays and own playlists are the ones Cratarr reads and keeps. The API has no remote control
//! of players, so this source cannot play on a device: Cratarr's own player plays its music (decision 0022).
//!
//! Track paths must be the server's real ones: Navidrome reports made-up "Artist/Album/01 - Title.ext" paths unless
//! "Report Real Path" is on for the player Cratarr logs in as (or `ND_SUBSONIC_DEFAULTREPORTREALPATH=true`). The
//! status says so when a sample path is not absolute.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use anyhow::{Context, Result, bail};
use md5::{Digest, Md5};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::{Choice, Field, FieldKind, Fut, Kind, Library, Playing, PlaylistEvent, Source, Status, Track, Values, text};

pub const KIND: Kind = Kind { id: "subsonic", label: "Subsonic", fields, connect };

/// The client name the server sees; Navidrome keeps its player settings (Report Real Path) under it.
const CLIENT: &str = "cratarr";
const API_VERSION: &str = "1.16.1";
/// Songs per request when listing a library.
const PAGE: usize = 500;

fn fields() -> Vec<Field> {
    vec![
        Field { key: "url", label: "Server address", kind: FieldKind::Url, required: true, help: None, bound_to: &[] },
        Field { key: "username", label: "User", kind: FieldKind::Text, required: true, help: None, bound_to: &[] },
        Field {
            key: "password",
            label: "Password",
            kind: FieldKind::Secret,
            required: true,
            help: Some("Its favourites, plays and own playlists are the ones Cratarr uses."),
            bound_to: &["url", "username"],
        },
    ]
}

fn connect(values: &Values, partial: bool) -> Result<Box<dyn Source>> {
    let url = text(values, "url").context("the server address is required")?.trim_end_matches('/');
    if !url.starts_with("http://") && !url.starts_with("https://") {
        bail!("the server address must start with http:// or https://");
    }
    let user = text(values, "username").context("the user is required")?;
    let password = text(values, "password").context("the password is required")?;
    let _ = partial; // nothing here waits on a choice
    Ok(Box::new(SubsonicSource { api: Subsonic::new(url, user, password)?, libraries: Mutex::new(HashMap::new()) }))
}

/// The Subsonic API: token login per request (md5 of password and a fresh salt), JSON answers.
struct Subsonic {
    http: reqwest::Client,
    base: String,
    user: String,
    password: String,
}

#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "subsonic-response")]
    response: serde_json::Value,
}

#[derive(Deserialize)]
struct ApiError {
    code: Option<i64>,
    message: Option<String>,
}

fn salt() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    format!("{:016x}", h.finish())
}

fn token(password: &str, salt: &str) -> String {
    Md5::digest(format!("{password}{salt}").as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

impl Subsonic {
    fn new(base: &str, user: &str, password: &str) -> Result<Self> {
        let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(20)).build()?;
        Ok(Self { http, base: base.trim_end_matches('/').to_string(), user: user.to_string(), password: password.to_string() })
    }

    fn request(&self, method: &str, query: &[(&str, String)]) -> reqwest::RequestBuilder {
        let s = salt();
        let auth = [
            ("u", self.user.clone()),
            ("t", token(&self.password, &s)),
            ("s", s),
            ("v", API_VERSION.to_string()),
            ("c", CLIENT.to_string()),
            ("f", "json".to_string()),
        ];
        self.http.get(format!("{}/rest/{method}", self.base)).query(&auth).query(query)
    }

    /// Errors without the address: it holds the login token.
    async fn send(&self, req: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        Ok(req.send().await.map_err(reqwest::Error::without_url)?)
    }

    /// The answer's `key` member (the whole answer for `""`), after checking it is not an error.
    async fn call<T: DeserializeOwned>(&self, method: &str, query: &[(&str, String)], key: &str) -> Result<T> {
        let resp = self.send(self.request(method, query)).await.with_context(|| method.to_string())?;
        let status = resp.status();
        if !status.is_success() {
            bail!("{method} returned {status}");
        }
        let env: Envelope = resp.json().await.map_err(reqwest::Error::without_url).with_context(|| format!("{method}: not a Subsonic answer"))?;
        let r = env.response;
        if r["status"].as_str() != Some("ok") {
            let e: ApiError = serde_json::from_value(r["error"].clone()).unwrap_or(ApiError { code: None, message: None });
            bail!("{}{}", e.message.unwrap_or_else(|| "the server refused".into()), e.code.map(|c| format!(" ({c})")).unwrap_or_default());
        }
        let v = if key.is_empty() { r } else { r.get(key).cloned().unwrap_or(serde_json::Value::Null) };
        serde_json::from_value(v).with_context(|| format!("{method}: unexpected answer"))
    }

    async fn ok(&self, method: &str, query: &[(&str, String)]) -> Result<()> {
        self.call::<serde_json::Value>(method, query, "").await.map(|_| ())
    }

    /// One page of every song in a folder (an empty search lists them all, in a stable order).
    async fn songs(&self, folder: &str, start: usize, limit: usize) -> Result<Vec<Song>> {
        self.songs_in(Some(folder), start, limit).await
    }

    /// One page of every song in a folder, or in every folder.
    async fn songs_in(&self, folder: Option<&str>, start: usize, limit: usize) -> Result<Vec<Song>> {
        #[derive(Deserialize, Default)]
        struct Found {
            #[serde(default)]
            song: Vec<Song>,
        }
        let mut q = vec![
            ("query", String::new()),
            ("songCount", limit.to_string()),
            ("songOffset", start.to_string()),
            ("albumCount", "0".into()),
            ("artistCount", "0".into()),
        ];
        if let Some(f) = folder {
            q.push(("musicFolderId", f.to_string()));
        }
        Ok(self.call::<Option<Found>>("search3", &q, "searchResult3").await?.unwrap_or_default().song)
    }

    async fn song(&self, id: &str) -> Result<Song> {
        self.call("getSong", &[("id", id.to_string())], "song").await
    }

    async fn album(&self, id: &str) -> Result<AlbumWithSongs> {
        self.call("getAlbum", &[("id", id.to_string())], "album").await
    }

    async fn folders(&self) -> Result<Vec<(String, String)>> {
        #[derive(Deserialize)]
        struct Folder {
            id: serde_json::Value,
            name: Option<String>,
        }
        #[derive(Deserialize, Default)]
        struct Folders {
            #[serde(rename = "musicFolder", default)]
            music_folder: Vec<Folder>,
        }
        let f: Option<Folders> = self.call("getMusicFolders", &[], "musicFolders").await?;
        Ok(f.unwrap_or_default()
            .music_folder
            .into_iter()
            .map(|f| {
                let id = f.id.as_str().map(str::to_string).unwrap_or_else(|| f.id.to_string());
                let name = f.name.unwrap_or_else(|| id.clone());
                (id, name)
            })
            .collect())
    }

    async fn starred_songs(&self) -> Result<Vec<Song>> {
        #[derive(Deserialize, Default)]
        struct Starred {
            #[serde(default)]
            song: Vec<Song>,
        }
        Ok(self.call::<Option<Starred>>("getStarred2", &[], "starred2").await?.unwrap_or_default().song)
    }

    async fn starred_albums(&self) -> Result<Vec<String>> {
        #[derive(Deserialize)]
        struct Album {
            id: String,
        }
        #[derive(Deserialize, Default)]
        struct Starred {
            #[serde(default)]
            album: Vec<Album>,
        }
        Ok(self.call::<Option<Starred>>("getStarred2", &[], "starred2").await?.unwrap_or_default().album.into_iter().map(|a| a.id).collect())
    }

    async fn starred_artists(&self) -> Result<Vec<String>> {
        #[derive(Deserialize)]
        struct Artist {
            name: String,
        }
        #[derive(Deserialize, Default)]
        struct Starred {
            #[serde(default)]
            artist: Vec<Artist>,
        }
        Ok(self.call::<Option<Starred>>("getStarred2", &[], "starred2").await?.unwrap_or_default().artist.into_iter().map(|a| a.name).collect())
    }

    async fn playlists(&self) -> Result<Vec<PlaylistHead>> {
        #[derive(Deserialize, Default)]
        struct Playlists {
            #[serde(default)]
            playlist: Vec<PlaylistHead>,
        }
        Ok(self.call::<Option<Playlists>>("getPlaylists", &[], "playlists").await?.unwrap_or_default().playlist)
    }

    async fn playlist_entries(&self, id: &str) -> Result<Vec<String>> {
        #[derive(Deserialize)]
        struct Full {
            #[serde(default)]
            entry: Vec<Song>,
        }
        let p: Full = self.call("getPlaylist", &[("id", id.to_string())], "playlist").await?;
        Ok(p.entry.into_iter().map(|s| s.id).collect())
    }

    async fn scan_status(&self) -> Result<ScanStatus> {
        self.call("getScanStatus", &[], "scanStatus").await
    }

    async fn cover(&self, id: &str, max_bytes: u64) -> Result<Option<(String, Vec<u8>)>> {
        let resp = self.send(self.request("getCoverArt", &[("id", id.to_string())])).await.context("getCoverArt")?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let resp = resp.error_for_status().map_err(reqwest::Error::without_url)?;
        let ct = resp.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        // An error comes back as a Subsonic answer, not an image.
        if !ct.starts_with("image/") {
            return Ok(None);
        }
        if resp.content_length().is_some_and(|n| n > max_bytes) {
            bail!("cover larger than {max_bytes} bytes");
        }
        let bytes = resp.bytes().await.map_err(reqwest::Error::without_url)?;
        if bytes.len() as u64 > max_bytes {
            bail!("cover larger than {max_bytes} bytes");
        }
        Ok(Some((ct, bytes.to_vec())))
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Song {
    id: String,
    #[serde(default)]
    title: String,
    album: Option<String>,
    artist: Option<String>,
    display_album_artist: Option<String>,
    album_id: Option<String>,
    path: Option<String>,
    year: Option<u32>,
    duration: Option<f64>,
    suffix: Option<String>,
    bit_rate: Option<u32>,
    bit_depth: Option<u32>,
    sampling_rate: Option<u32>,
    play_count: Option<u32>,
    starred: Option<String>,
    explicit_status: Option<String>,
    /// When last played (OpenSubsonic).
    played: Option<String>,
    /// 1 to 5.
    user_rating: Option<u8>,
    /// MusicBrainz's recording (OpenSubsonic).
    music_brainz_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlbumWithSongs {
    id: String,
    #[serde(default)]
    name: String,
    artist: Option<String>,
    year: Option<u32>,
    created: Option<String>,
    #[serde(default)]
    song: Vec<Song>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaylistHead {
    id: String,
    name: String,
    owner: Option<String>,
    changed: Option<String>,
    song_count: Option<u32>,
    /// Navidrome: a playlist imported from a file (it follows the file and cannot be edited).
    #[serde(default)]
    readonly: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScanStatus {
    #[serde(default)]
    scanning: bool,
    last_scan: Option<String>,
}

/// The folder an album's files are in: the folder their own folders share (an album with a folder per disc is the
/// folder above them).
fn album_folder<'a>(paths: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let mut common: Option<Vec<&str>> = None;
    for p in paths {
        let parts: Vec<&str> = p.split('/').collect();
        let dir = &parts[..parts.len().saturating_sub(1)];
        common = Some(match common {
            None => dir.to_vec(),
            Some(c) => c.iter().zip(dir).take_while(|(a, b)| a == b).map(|(a, _)| *a).collect(),
        });
    }
    common.map(|c| c.join("/")).filter(|f| !f.is_empty())
}

/// An album's track count, format (one for all, upper case), lowest bitrate and length.
fn summarize(songs: &[&Song]) -> super::AlbumDetails {
    let formats: std::collections::BTreeSet<String> = songs.iter().filter_map(|s| s.suffix.as_deref()).map(str::to_uppercase).collect();
    super::AlbumDetails {
        tracks: songs.len() as u32,
        format: (formats.len() == 1).then(|| formats.into_iter().next().unwrap_or_default()),
        bitrate_kbps: songs.iter().filter_map(|s| s.bit_rate).filter(|b| *b > 0).min(),
        duration_secs: super::total_secs(songs.iter().map(|s| s.duration)),
    }
}

fn album_track(s: Song) -> super::AlbumTrack {
    super::AlbumTrack {
        id: s.id,
        title: s.title,
        artist: s.artist,
        album: s.album,
        album_artist: s.display_album_artist,
        year: s.year,
        path: s.path,
        duration_secs: s.duration,
    }
}

fn epoch_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

struct SubsonicSource {
    api: Subsonic,
    /// Which folder a track is in, by track id: the API does not say, so it is looked up once (see `library_of`).
    libraries: Mutex<HashMap<String, Option<String>>>,
}

impl SubsonicSource {
    /// The folder a song is in. With one folder, that one; with several, the one whose search for its title finds it.
    async fn library_of(&self, song: &Song) -> Option<String> {
        if let Some(hit) = self.libraries.lock().unwrap_or_else(|p| p.into_inner()).get(&song.id) {
            return hit.clone();
        }
        let folders = self.api.folders().await.ok()?;
        let found = if let [(only, _)] = folders.as_slice() {
            Some(only.clone())
        } else {
            let mut found = None;
            for (id, _) in &folders {
                let q = [
                    ("query", song.title.clone()),
                    ("songCount", "100".into()),
                    ("albumCount", "0".into()),
                    ("artistCount", "0".into()),
                    ("musicFolderId", id.clone()),
                ];
                #[derive(Deserialize, Default)]
                struct Found {
                    #[serde(default)]
                    song: Vec<Song>,
                }
                let hits: Option<Found> = self.api.call("search3", &q, "searchResult3").await.ok()?;
                if hits.unwrap_or_default().song.iter().any(|s| s.id == song.id) {
                    found = Some(id.clone());
                    break;
                }
            }
            found
        };
        let mut cache = self.libraries.lock().unwrap_or_else(|p| p.into_inner());
        if cache.len() > 4096 {
            cache.clear();
        }
        cache.insert(song.id.clone(), found.clone());
        found
    }

    async fn to_track(&self, s: Song) -> Track {
        let library_id = self.library_of(&s).await;
        Track { id: s.id, title: s.title, artist: s.display_album_artist.or(s.artist), album: s.album, album_id: s.album_id, path: s.path, library_id }
    }

    /// Every song in a folder, in pages.
    async fn all_songs(&self, folder: &str) -> Result<Vec<Song>> {
        self.all_songs_in(Some(folder)).await
    }

    /// Every song in a folder, or in every folder, in pages.
    async fn all_songs_in(&self, folder: Option<&str>) -> Result<Vec<Song>> {
        let mut out = Vec::new();
        loop {
            let page = self.api.songs_in(folder, out.len(), PAGE).await?;
            let n = page.len();
            out.extend(page);
            if n < PAGE {
                return Ok(out);
            }
        }
    }

    /// The playlists this account keeps itself (not the ones the server imported from files).
    async fn own(&self) -> Result<Vec<PlaylistHead>> {
        Ok(self.api.playlists().await?.into_iter().filter(|p| !p.readonly && p.owner.as_deref().is_none_or(|o| o == self.api.user)).collect())
    }

    /// Have the server look at its folders now (a quick scan: changed folders only).
    async fn rescan(&self) -> Result<()> {
        self.api.ok("startScan", &[]).await
    }

    /// The API shows the plays and stars of the account that logs in, nobody else's.
    fn own_login(&self, user: &str) -> Result<()> {
        if user != self.api.user {
            bail!("a Subsonic server shows only its own login's plays and favourites");
        }
        Ok(())
    }
}

/// A song's plays, star and rating as an import reads them; `None` when it has none of them.
fn song_data(s: Song) -> Option<super::TrackData> {
    let plays = s.play_count.unwrap_or(0);
    let rating = s.user_rating.unwrap_or(0).min(5);
    if plays == 0 && s.starred.is_none() && rating == 0 {
        return None;
    }
    let epoch = |d: Option<&str>| d.and_then(crate::nowplaying::parse_iso_epoch).filter(|t| *t > 0);
    // Its names, for an import that finds it by them (decision 0067).
    let meta = super::TrackMeta {
        artist: s.artist.clone().or_else(|| s.display_album_artist.clone()).unwrap_or_default(),
        title: s.title.clone(),
        album: s.album.clone(),
        length_secs: s.duration.filter(|d| *d > 0.0),
        recording: s.music_brainz_id.clone().filter(|m| !m.is_empty()),
    };
    Some(super::TrackData {
        last_played: epoch(s.played.as_deref()),
        favorite: s.starred.is_some(),
        favorite_at: epoch(s.starred.as_deref()),
        id: s.id,
        plays,
        rating,
        meta: Some(meta),
    })
}

impl Source for SubsonicSource {
    fn status(&self) -> Fut<'_, Status> {
        Box::pin(async move {
            let about = match self.api.call::<serde_json::Value>("ping", &[], "").await {
                Ok(v) => v,
                Err(e) => return Ok(Status { error: Some(format!("cannot log in: {e:#}")), ..Status::default() }),
            };
            let name = about["type"].as_str().map(|t| {
                let mut c = t.chars();
                c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
            });
            let version = about["serverVersion"].as_str().or(about["version"].as_str()).map(str::to_string);
            // Made-up paths cannot be placed: say how to get the real ones.
            let mut error = None;
            if let Ok(folders) = self.api.folders().await
                && let Some((id, _)) = folders.first()
                && let Ok(songs) = self.api.songs(id, 0, 1).await
                && songs.first().and_then(|s| s.path.as_deref()).is_some_and(|p| !p.starts_with('/'))
            {
                error = Some(format!("the server reports made-up paths: turn on Report Real Path for the player \"{CLIENT}\""));
            }
            Ok(Status { connected: error.is_none(), server_name: name, version, detail: Some(format!("as {}", self.api.user)), error })
        })
    }

    fn options(&self) -> Fut<'_, BTreeMap<String, Vec<Choice>>> {
        Box::pin(async { Ok(BTreeMap::new()) })
    }

    fn libraries(&self) -> Fut<'_, Vec<Library>> {
        Box::pin(async move { Ok(self.api.folders().await?.into_iter().map(|(id, name)| Library { id, name, paths: Vec::new() }).collect()) })
    }

    fn sample_paths<'a>(&'a self, library_id: &'a str, n: usize) -> Fut<'a, Vec<String>> {
        Box::pin(async move { Ok(self.api.songs(library_id, 0, n).await?.into_iter().filter_map(|s| s.path).collect()) })
    }

    fn now_playing(&self) -> Fut<'_, Option<Playing>> {
        Box::pin(async move {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Entry {
                #[serde(flatten)]
                song: Song,
                username: Option<String>,
                minutes_ago: Option<i64>,
                player_name: Option<String>,
            }
            #[derive(Deserialize, Default)]
            struct NowPlaying {
                #[serde(default)]
                entry: Vec<Entry>,
            }
            let np: Option<NowPlaying> = self.api.call("getNowPlaying", &[], "nowPlaying").await?;
            // This account's newest; the server reports no position, only minutes since it started.
            let Some(e) = np
                .unwrap_or_default()
                .entry
                .into_iter()
                .filter(|e| e.username.as_deref().is_none_or(|u| u == self.api.user))
                .min_by_key(|e| e.minutes_ago.unwrap_or(i64::MAX))
            else {
                return Ok(None);
            };
            let favorite = Some(e.song.starred.is_some());
            let started_at = e.minutes_ago.map(|m| epoch_now() - m * 60);
            let track = self.to_track(e.song).await;
            Ok(Some(Playing { track, progress: None, player: e.player_name, started_at, favorite, upcoming: Vec::new() }))
        })
    }

    fn track<'a>(&'a self, id: &'a str) -> Fut<'a, Track> {
        Box::pin(async move {
            let s = self.api.song(id).await?;
            Ok(self.to_track(s).await)
        })
    }

    /// The server finds playlist files itself when it scans; a quick scan makes that now.
    fn playlists_changed(&self, events: Vec<PlaylistEvent>) -> Fut<'_, ()> {
        Box::pin(async move {
            if !events.is_empty() {
                if let Err(e) = self.rescan().await {
                    tracing::warn!("could not ask the Subsonic server to look at the playlist files: {e:#}");
                }
            }
            Ok(())
        })
    }

    fn files_changed<'a>(&'a self, changes: &'a [(String, bool)]) -> Fut<'a, ()> {
        Box::pin(async move { if changes.is_empty() { Ok(()) } else { self.rescan().await } })
    }

    fn library_scan(&self) -> Fut<'_, super::LibraryScan> {
        Box::pin(async move {
            let s = self.api.scan_status().await?;
            let last = s.last_scan.as_deref().and_then(crate::nowplaying::parse_iso_epoch);
            Ok(super::LibraryScan { running: s.scanning.then_some(0.0), last_end: if s.scanning { None } else { last } })
        })
    }

    fn lists_tracks(&self) -> bool {
        true
    }

    /// The API cannot list what changed since a moment: everything again after a scan since then, else nothing.
    fn track_paths<'a>(&'a self, library_id: &'a str, since: Option<i64>, start: usize, limit: usize) -> Fut<'a, super::TrackPage> {
        Box::pin(async move {
            if let Some(t) = since {
                let s = self.api.scan_status().await?;
                let last = s.last_scan.as_deref().and_then(crate::nowplaying::parse_iso_epoch);
                if !s.scanning && last.is_some_and(|l| l < t) {
                    return Ok(super::TrackPage::default());
                }
            }
            let songs = self.api.songs(library_id, start, limit).await?;
            let n = songs.len();
            // The total is not told: a full page means there may be more.
            let total = if n < limit { start + n } else { usize::MAX };
            Ok(super::TrackPage { items: songs.into_iter().filter_map(|s| s.path.map(|p| (s.id, p))).collect(), total })
        })
    }

    fn lists_albums(&self) -> bool {
        true
    }

    /// Albums newest first. The album list gives no folders, so the folder's songs are listed too (5,400 songs in
    /// eleven requests), which also gives each album's track count, format and bitrate.
    fn albums<'a>(&'a self, library_id: &'a str, limit: usize) -> Fut<'a, Vec<super::Listed>> {
        Box::pin(async move {
            #[derive(Deserialize, Default)]
            struct List {
                #[serde(default)]
                album: Vec<AlbumWithSongs>,
            }
            let mut heads: Vec<AlbumWithSongs> = Vec::new();
            while heads.len() < limit {
                let want = PAGE.min(limit - heads.len());
                let q = [
                    ("type", "newest".to_string()),
                    ("size", want.to_string()),
                    ("offset", heads.len().to_string()),
                    ("musicFolderId", library_id.to_string()),
                ];
                let page = self.api.call::<Option<List>>("getAlbumList2", &q, "albumList2").await?.unwrap_or_default().album;
                let n = page.len();
                heads.extend(page);
                if n < want {
                    break;
                }
            }
            let songs = self.all_songs(library_id).await?;
            let mut by_album: HashMap<&str, Vec<&Song>> = HashMap::new();
            for s in &songs {
                if let Some(a) = s.album_id.as_deref() {
                    by_album.entry(a).or_default().push(s);
                }
            }
            Ok(heads
                .into_iter()
                .map(|a| {
                    let own = by_album.get(a.id.as_str()).map(Vec::as_slice).unwrap_or_default();
                    let d = summarize(own);
                    let path = album_folder(own.iter().filter_map(|s| s.path.as_deref()));
                    // Changes when a track is added, removed or re-read.
                    let stamp = format!("{} {} {}", d.tracks, a.created.as_deref().unwrap_or(""), path.as_deref().unwrap_or(""));
                    let album = super::Album {
                        added: a.created.as_deref().and_then(crate::nowplaying::parse_iso_epoch),
                        title: a.name,
                        artist: a.artist,
                        year: a.year,
                        path,
                        tags: Vec::new(),
                        library: None,
                        artists: Vec::new(),
                        release: Default::default(),
                        id: a.id,
                        tracks: d.tracks,
                        format: d.format,
                        bitrate_kbps: d.bitrate_kbps,
                        duration_secs: d.duration_secs,
                    };
                    super::Listed { album, stamp: Some(stamp) }
                })
                .collect())
        })
    }

    fn album_summary<'a>(&'a self, album: &'a str) -> Fut<'a, Option<super::AlbumDetails>> {
        Box::pin(async move {
            let a = self.api.album(album).await?;
            Ok(Some(summarize(&a.song.iter().collect::<Vec<_>>())))
        })
    }

    fn tracks_of_album<'a>(&'a self, album_id: &'a str) -> Fut<'a, Vec<super::AlbumTrack>> {
        Box::pin(async move { Ok(self.api.album(album_id).await?.song.into_iter().map(album_track).collect()) })
    }

    fn users(&self) -> Fut<'_, Vec<(String, String)>> {
        Box::pin(async move { Ok(vec![(self.api.user.clone(), self.api.user.clone())]) })
    }

    fn sync_account(&self) -> Fut<'_, String> {
        Box::pin(async move { Ok(self.api.user.clone()) })
    }

    fn own_playlists<'a>(&'a self, _user: &'a str) -> Fut<'a, Vec<super::OwnPlaylist>> {
        Box::pin(async move {
            Ok(self
                .own()
                .await?
                .into_iter()
                .map(|p| super::OwnPlaylist { stamp: Some(format!("{} {}", p.changed.unwrap_or_default(), p.song_count.unwrap_or(0))), id: p.id, name: p.name })
                .collect())
        })
    }

    fn playlist_tracks<'a>(&'a self, _user: &'a str, playlist: &'a str) -> Fut<'a, Vec<String>> {
        Box::pin(self.api.playlist_entries(playlist))
    }

    fn create_playlist<'a>(&'a self, _user: &'a str, name: &'a str, ids: &'a [String]) -> Fut<'a, String> {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct Made {
                id: String,
            }
            let mut q = vec![("name", name.to_string())];
            q.extend(ids.iter().map(|i| ("songId", i.clone())));
            let made: Option<Made> = self.api.call("createPlaylist", &q, "playlist").await?;
            if let Some(m) = made {
                return Ok(m.id);
            }
            // Servers on API 1.13 and older answer without the playlist: find it by name.
            self.own().await?.into_iter().rev().find(|p| p.name == name).map(|p| p.id).context("the new playlist is not there")
        })
    }

    /// `createPlaylist` with an id replaces the list in one go.
    fn set_playlist_tracks<'a>(&'a self, _user: &'a str, playlist: &'a str, ids: &'a [String]) -> Fut<'a, ()> {
        Box::pin(async move {
            let mut q = vec![("playlistId", playlist.to_string())];
            q.extend(ids.iter().map(|i| ("songId", i.clone())));
            self.api.ok("createPlaylist", &q).await
        })
    }

    fn rename_playlist<'a>(&'a self, _user: &'a str, playlist: &'a str, name: &'a str) -> Fut<'a, String> {
        Box::pin(async move {
            self.api.ok("updatePlaylist", &[("playlistId", playlist.to_string()), ("name", name.to_string())]).await?;
            Ok(playlist.to_string())
        })
    }

    fn delete_playlist<'a>(&'a self, playlist: &'a str) -> Fut<'a, ()> {
        Box::pin(async move { self.api.ok("deletePlaylist", &[("id", playlist.to_string())]).await })
    }

    fn track_info<'a>(&'a self, track: &'a str) -> Fut<'a, Option<super::TrackInfo>> {
        Box::pin(async move {
            let s = self.api.song(track).await?;
            Ok(Some(super::TrackInfo {
                play_count: s.play_count,
                codec: s.suffix.map(|c| c.to_uppercase()),
                bitrate_kbps: s.bit_rate.filter(|b| *b > 0),
                bit_depth: s.bit_depth.filter(|b| *b > 0),
                sample_rate_hz: s.sampling_rate.filter(|r| *r > 0),
                explicit: match s.explicit_status.as_deref() {
                    Some("explicit") => Some(true),
                    Some("clean") => Some(false),
                    _ => None,
                },
                bpm: None,
            }))
        })
    }

    fn count_play<'a>(&'a self, track: &'a str) -> Fut<'a, ()> {
        Box::pin(async move { self.api.ok("scrobble", &[("id", track.to_string()), ("submission", "true".into())]).await })
    }

    fn favorites<'a>(&'a self, tracks: &'a [String]) -> Fut<'a, BTreeMap<String, bool>> {
        Box::pin(async move {
            if tracks.is_empty() {
                return Ok(BTreeMap::new());
            }
            let starred: std::collections::HashSet<String> = self.api.starred_songs().await?.into_iter().map(|s| s.id).collect();
            Ok(tracks.iter().map(|t| (t.clone(), starred.contains(t))).collect())
        })
    }

    fn favorite_tracks(&self) -> Fut<'_, Vec<String>> {
        Box::pin(async move { Ok(self.api.starred_songs().await?.into_iter().map(|s| s.id).collect()) })
    }

    fn favorite<'a>(&'a self, track: &'a str) -> Fut<'a, Option<bool>> {
        Box::pin(async move { Ok(Some(self.api.song(track).await?.starred.is_some())) })
    }

    fn set_favorite<'a>(&'a self, track: &'a str, on: bool) -> Fut<'a, bool> {
        Box::pin(async move {
            self.api.ok(if on { "star" } else { "unstar" }, &[("id", track.to_string())]).await?;
            Ok(on)
        })
    }

    fn favorite_albums(&self) -> Fut<'_, Vec<String>> {
        Box::pin(self.api.starred_albums())
    }

    fn favorite_artists(&self) -> Fut<'_, Vec<String>> {
        Box::pin(self.api.starred_artists())
    }

    fn set_album_favorite<'a>(&'a self, album: &'a str, on: bool) -> Fut<'a, bool> {
        Box::pin(async move {
            self.api.ok(if on { "star" } else { "unstar" }, &[("albumId", album.to_string())]).await?;
            Ok(on)
        })
    }

    fn album_cover<'a>(&'a self, album_id: &'a str) -> Fut<'a, Option<(String, Vec<u8>)>> {
        Box::pin(self.api.cover(album_id, crate::covers::MAX_BYTES))
    }

    fn imports(&self) -> bool {
        true
    }

    /// Every song of every folder (an empty search), keeping the ones played, starred or rated: the API cannot list
    /// only those.
    fn track_data<'a>(&'a self, user: &'a str, cursor: Option<&'a str>) -> Fut<'a, super::TrackDataPage> {
        Box::pin(async move {
            self.own_login(user)?;
            let start: usize = match cursor {
                Some(c) => c.parse().context("not a cursor")?,
                None => 0,
            };
            let songs = self.api.songs_in(None, start, PAGE).await?;
            let n = songs.len();
            let items = songs.into_iter().filter_map(song_data).collect();
            Ok(super::TrackDataPage { items, next: (n == PAGE).then(|| (start + n).to_string()), total: None })
        })
    }

    /// The starred albums name no song or folder, so each album is asked for its own (one request each); one that
    /// cannot be read is passed on without, and the import counts it as not found.
    fn favorite_groups<'a>(&'a self, user: &'a str) -> Fut<'a, super::FavoriteGroups> {
        Box::pin(async move {
            self.own_login(user)?;
            #[derive(Deserialize)]
            struct Starred {
                id: String,
                name: Option<String>,
                artist: Option<String>,
                starred: Option<String>,
            }
            #[derive(Deserialize, Default)]
            struct All {
                #[serde(default)]
                album: Vec<Starred>,
                #[serde(default)]
                artist: Vec<Starred>,
            }
            let all = self.api.call::<Option<All>>("getStarred2", &[], "starred2").await?.unwrap_or_default();
            let at = |d: Option<&str>| d.and_then(crate::nowplaying::parse_iso_epoch).filter(|t| *t > 0);
            let mut groups = super::FavoriteGroups::default();
            for a in all.album {
                let songs = self.api.album(&a.id).await.map(|x| x.song).unwrap_or_default();
                let path = album_folder(songs.iter().filter_map(|s| s.path.as_deref()));
                let track = songs.into_iter().next().map(|s| s.id);
                groups.albums.push((super::AlbumRef { id: a.id, track, path, name: a.name, artist: a.artist }, at(a.starred.as_deref())));
            }
            for a in all.artist {
                if let Some(name) = a.name.filter(|n| !n.trim().is_empty()) {
                    groups.artists.push((super::ArtistRef { id: a.id, name }, at(a.starred.as_deref())));
                }
            }
            Ok(groups)
        })
    }

    /// Every album at once, in one page: the album list names no song or folder, so every song is listed as well (a
    /// request per 500) to find each album's first song and folder.
    fn albums_added<'a>(&'a self, _cursor: Option<&'a str>) -> Fut<'a, super::AddedPage> {
        Box::pin(async move {
            #[derive(Deserialize, Default)]
            struct List {
                #[serde(default)]
                album: Vec<AlbumWithSongs>,
            }
            let mut heads: Vec<AlbumWithSongs> = Vec::new();
            loop {
                let q = [("type", "newest".to_string()), ("size", PAGE.to_string()), ("offset", heads.len().to_string())];
                let page = self.api.call::<Option<List>>("getAlbumList2", &q, "albumList2").await?.unwrap_or_default().album;
                let n = page.len();
                heads.extend(page);
                if n < PAGE {
                    break;
                }
            }
            let songs = self.all_songs_in(None).await?;
            let mut by_album: HashMap<&str, Vec<&Song>> = HashMap::new();
            for s in &songs {
                if let Some(a) = s.album_id.as_deref() {
                    by_album.entry(a).or_default().push(s);
                }
            }
            let total = heads.len();
            let items = heads
                .into_iter()
                .filter_map(|a| {
                    let at = a.created.as_deref().and_then(crate::nowplaying::parse_iso_epoch).filter(|t| *t > 0)?;
                    let own = by_album.get(a.id.as_str()).map(Vec::as_slice).unwrap_or_default();
                    let path = album_folder(own.iter().filter_map(|s| s.path.as_deref()));
                    let track = own.first().map(|s| s.id.clone());
                    Some((super::AlbumRef { id: a.id, track, path, name: Some(a.name).filter(|n| !n.is_empty()), artist: a.artist }, at))
                })
                .collect();
            Ok(super::AddedPage { items, next: None, total: Some(total) })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_token_is_the_md5_of_password_and_salt() {
        // The Subsonic API documentation's own example.
        assert_eq!(token("sesame", "c19b2d"), "26719a1196d2a940705a59634eb18eab");
    }

    #[test]
    fn an_albums_folder_is_the_one_its_files_share() {
        assert_eq!(album_folder(["/music/A/X (2001)/01.flac", "/music/A/X (2001)/02.flac"]), Some("/music/A/X (2001)".into()));
        assert_eq!(album_folder(["/music/A/X/CD1/01.flac", "/music/A/X/CD2/01.flac"]), Some("/music/A/X".into()));
        assert_eq!(album_folder(Vec::<&str>::new()), None);
    }

    #[test]
    fn answers_read_as_songs() {
        let s: Song = serde_json::from_str(
            r#"{"id":"6CN","title":"Music Is Math","album":"Geogaddi","artist":"Boards of Canada","albumId":"05P",
                "path":"/music/Boards of Canada/Geogaddi (2002)/01 Music Is Math.mp3","suffix":"mp3","bitRate":64,
                "bitDepth":0,"samplingRate":44100,"duration":4,"year":2002,"starred":"2026-10-15T12:00:00Z"}"#,
        )
        .unwrap();
        assert_eq!(s.album_id.as_deref(), Some("05P"));
        assert!(s.starred.is_some());
        let d = summarize(&[&s]);
        assert_eq!((d.tracks, d.format.as_deref(), d.bitrate_kbps, d.duration_secs), (1, Some("MP3"), Some(64), Some(4.0)));
    }

    #[test]
    fn an_import_keeps_the_songs_played_starred_or_rated() {
        let song = |extra: &str| -> Song { serde_json::from_str(&format!(r#"{{"id":"1","title":"t"{extra}}}"#)).unwrap() };
        assert_eq!(song_data(song("")), None);
        assert_eq!(song_data(song(r#","playCount":0,"userRating":0"#)), None);
        let played = song_data(song(r#","playCount":3,"played":"2026-10-12T13:28:00Z""#)).unwrap();
        assert_eq!((played.plays, played.last_played, played.favorite), (3, Some(1_791_811_680), false));
        let starred = song_data(song(r#","starred":"2026-10-12T13:28:00Z","userRating":4"#)).unwrap();
        assert_eq!((starred.plays, starred.favorite, starred.favorite_at, starred.rating), (0, true, Some(1_791_811_680), 4));
    }
}
