//! Thin Jellyfin API client. Jellyfin supplies what is playing and is told when a playlist file changed; the
//! playlists themselves are `.m3u` files (see `playlists.rs`). Unknown fields are ignored so harmless additions
//! by Jellyfin never break us, while the fields we rely on are required and fail loudly.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ItemsPage<T> {
    pub items: Vec<T>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Session {
    pub id: Option<String>,
    /// The account the player is signed in with.
    pub user_id: Option<String>,
    /// Jellyfin sends these two; nothing reads them now (the tests build sessions with them), kept as its API has them.
    #[allow(dead_code)]
    pub supports_remote_control: Option<bool>,
    /// Remote commands the client says it takes. Finamp reports `SupportsRemoteControl: false` yet lists commands
    /// here and does play what it is sent (observed on 10.11), so a non-empty list counts as controllable too.
    #[allow(dead_code)]
    pub supported_commands: Option<Vec<String>>,
    pub last_activity_date: Option<String>,
    /// When the player last reported playback (start, progress or stop); `0001-01-01…` when it never did.
    pub last_playback_check_in: Option<String>,
    pub client: Option<String>,
    pub device_name: Option<String>,
    pub now_playing_item: Option<PlayingItem>,
    pub play_state: Option<PlayState>,
    /// What the player plays after the current track, in order, as far as it reports (Finamp: the next few, never
    /// the ones before; seen 2026-10-13).
    #[serde(default)]
    pub now_playing_queue_full_items: Vec<PlayingItem>,
}

/// A device Jellyfin remembers. Note: a session's `Client` and `DeviceId` do not always match the device's
/// `AppName` and `Id` (observed: Finamp sessions with ids missing from this list, "Jellyfin for Android" vs
/// "Jellyfin Android"), so players are matched by app and device name, loosely (see `source::jellyfin`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Device {
    pub app_name: Option<String>,
    pub name: Option<String>,
    pub date_last_activity: Option<String>,
}

/// A library as configured in Jellyfin. Its `Locations` are only shown, never used to find files: they need not
/// match the paths Jellyfin reports for the tracks in it (observed: `/Volume1/media/Music` vs `/data/media/Music/...`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct VirtualFolder {
    pub name: String,
    pub collection_type: Option<String>,
    pub item_id: String,
    #[serde(default)]
    pub locations: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct UserLite {
    id: String,
    policy: Option<PolicyLite>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PolicyLite {
    is_administrator: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Ancestor {
    id: String,
    #[serde(rename = "Type")]
    kind: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct IdOnly {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct NameOnly {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ListedAlbumItem {
    id: String,
    name: Option<String>,
    album_artist: Option<String>,
    production_year: Option<u32>,
    date_created: Option<String>,
    path: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    /// Changes when Jellyfin saves the album again (rescan, tag edit): the cue to read its tracks again.
    date_last_saved: Option<String>,
    /// Its tracks' lengths summed by Jellyfin (an unknown one as nothing); sent without asking.
    run_time_ticks: Option<i64>,
}

/// What an album's tracks add up to.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AlbumSummary {
    pub tracks: u32,
    /// When every track has the same container; `None` when mixed or unknown.
    pub format: Option<String>,
    /// The lowest track bitrate, so a weak track is not hidden by an average.
    pub bitrate_kbps: Option<u32>,
    /// Their lengths summed, in seconds; `None` when any is unknown (`source::total_secs`).
    pub duration_secs: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ListedAudio {
    container: Option<String>,
    media_sources: Option<Vec<ListedSource>>,
    run_time_ticks: Option<i64>,
}

/// Jellyfin's ticks (100 ns) in seconds.
fn ticks_to_secs(ticks: i64) -> f64 {
    ticks as f64 / 10_000_000.0
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PathOnly {
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct UserName {
    id: String,
    name: String,
}

/// Ids per playlist request: they travel in the query string, which Jellyfin caps near 8 KB (decision 0006: 240 ids).
const PLAYLIST_BATCH: usize = 200;

/// A playlist's entries without images and user data: only ids are read, and the full answer for a 721-track
/// playlist took 98 s on a busy server against 48 s for this (2026-10-15), past the 20 s every request gets.
const PLAYLIST_ITEMS_LIGHT: &str = "&Fields=&EnableImages=false&EnableUserData=false";

/// How long a long read may take (`get_long`): the light read above still took 48 s there.
const LONG_READ: std::time::Duration = std::time::Duration::from_secs(180);

/// A playlist as a user's listing shows it.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct UserPlaylist {
    pub id: String,
    pub name: String,
    /// Changes when the playlist is saved.
    pub etag: Option<String>,
    pub path: Option<String>,
}

/// Where a session is in its track. Observed on 10.11 with Finamp: `PositionTicks` is current whenever it is read
/// (it advanced 2 s per 2 s poll between the client's check-ins), so no extrapolation from report times is needed.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlayState {
    pub position_ticks: Option<i64>,
    pub is_paused: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct PlayingItem {
    pub id: String,
    pub name: String,
    pub path: Option<String>,
    pub album: Option<String>,
    pub album_id: Option<String>,
    pub album_artist: Option<String>,
    pub media_type: Option<String>,
    /// Track length; 10 000 000 ticks per second.
    pub run_time_ticks: Option<i64>,
}

/// One of an album's tracks, as its view lists them.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AlbumTrackItem {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub artists: Vec<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub production_year: Option<u32>,
    pub path: Option<String>,
    /// Track length; 10 000 000 ticks per second.
    pub run_time_ticks: Option<i64>,
}

/// A track's file as Jellyfin describes it, and the asking user's data on it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MediaDetails {
    #[serde(default)]
    pub media_streams: Vec<MediaStream>,
    pub user_data: Option<UserData>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MediaStream {
    #[serde(rename = "Type")]
    pub kind: Option<String>,
    pub codec: Option<String>,
    /// Bits per second.
    pub bit_rate: Option<u64>,
    pub bit_depth: Option<u32>,
    pub sample_rate: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserData {
    pub play_count: Option<u32>,
}

/// One row of Jellyfin's activity log; only audio playback start/stop entries matter to us.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ActivityEntry {
    pub date: String,
    #[serde(rename = "Type")]
    pub kind: String,
    pub item_id: Option<String>,
}

/// A playlist as listed by Jellyfin (the cheap listing; reading a playlist's entries is the slow part).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ListedPlaylist {
    pub id: String,
    pub name: String,
}

/// An album as Jellyfin lists it; `path` is the album folder as Jellyfin sees it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ListedAlbum {
    pub id: String,
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ListedTrack {
    pub path: Option<String>,
    pub run_time_ticks: Option<i64>,
    pub media_sources: Option<Vec<ListedSource>>,
    pub user_data: Option<ListedUserData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ListedSource {
    pub bitrate: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ListedUserData {
    pub play_count: Option<u32>,
    pub is_favorite: Option<bool>,
}

pub fn pick_playlist_id(items: &[ListedPlaylist], name: &str) -> Option<String> {
    items.iter().find(|p| p.name == name).map(|p| p.id.clone())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PublicInfo {
    pub version: String,
    pub server_name: Option<String>,
}

/// Fallback source for "what is playing": audio sessions only, preferring the chosen player.
pub fn pick_now_playing(sessions: &[Session], prefers: impl Fn(&Session) -> bool) -> Option<PlayingItem> {
    let audio = |s: &&Session| {
        s.now_playing_item
            .as_ref()
            .is_some_and(|i| i.media_type.as_deref().is_some_and(|m| m.eq_ignore_ascii_case("Audio")))
    };
    let preferred = sessions
        .iter()
        .filter(audio)
        .find(|s| prefers(s));
    preferred
        .or_else(|| sessions.iter().find(audio))
        .and_then(|s| s.now_playing_item.clone())
}

/// Epoch seconds as the UTC date-time Jellyfin's query parameters take.
fn iso(t: i64) -> String {
    let days = t.div_euclid(86_400);
    let secs = t.rem_euclid(86_400);
    // civil from days (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", secs / 3600, secs % 3600 / 60, secs % 60)
}

/// At most this many requests to Jellyfin at a time; the rest wait their turn. Jellyfin slows down from many heavy
/// queries at once, and on 2026-10-14 a page asking in a loop went straight through to it (hours at 200-300 % CPU).
const MAX_IN_FLIGHT: usize = 4;
/// How long a request waits for its turn before it gives up.
const TURN_WAIT: std::time::Duration = std::time::Duration::from_secs(20);

/// One GET's answer (the body, or the error as text), shared by everyone who asked for it meanwhile.
type Answer = Arc<tokio::sync::OnceCell<std::result::Result<Arc<[u8]>, String>>>;

#[derive(Clone)]
pub struct Jellyfin {
    http: reqwest::Client,
    base: String,
    turns: Arc<tokio::sync::Semaphore>,
    /// GETs being answered now, by path: a second asker of the same path waits for that answer instead of asking again.
    asking: Arc<Mutex<HashMap<String, Answer>>>,
}

impl Jellyfin {
    /// No Jellyfin user is involved: the API key (admin rights) is enough for every call, and Cratarr's playlists
    /// are not per user (decision 0005).
    pub fn new(base: &str, api_key: &str) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        let mut token = reqwest::header::HeaderValue::from_str(api_key).context("API key has invalid characters")?;
        token.set_sensitive(true);
        headers.insert("X-Emby-Token", token);
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(20))
            .build()?;
        Ok(Self {
            http,
            base: base.trim_end_matches('/').to_string(),
            turns: Arc::new(tokio::sync::Semaphore::new(MAX_IN_FLIGHT)),
            asking: Arc::default(),
        })
    }

    /// Send a request when it is its turn (`MAX_IN_FLIGHT`). The turn ends when the answer's headers are in. After a
    /// change (anything but a GET) a GET asks again rather than share an answer that may predate the change.
    async fn send(&self, req: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        let req = req.build()?;
        let _turn = tokio::time::timeout(TURN_WAIT, self.turns.acquire())
            .await
            .map_err(|_| anyhow::anyhow!("Jellyfin is busy (no turn within {} s)", TURN_WAIT.as_secs()))?
            .expect("the semaphore is never closed");
        let changes = req.method() != reqwest::Method::GET;
        let resp = self.http.execute(req).await;
        if changes {
            self.asking.lock().unwrap().clear();
        }
        Ok(resp?)
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        self.get_within(path, None).await
    }

    /// `get` for an answer a busy server takes long over (a long playlist's entries, a page of a library's albums or tracks:
/// 22 to 29 s for 250 albums while Jellyfin was near its memory limit, 2026-10-15): `LONG_READ` instead of 20 s.
    async fn get_long<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        self.get_within(path, Some(LONG_READ)).await
    }

    async fn get_within<T: for<'de> Deserialize<'de>>(&self, path: &str, timeout: Option<std::time::Duration>) -> Result<T> {
        let body = self.get_shared(path, timeout).await.map_err(anyhow::Error::msg)?;
        serde_json::from_slice(&body).with_context(|| format!("GET {path}: response did not have the expected shape"))
    }

    /// The body of `GET path`, asked once however many ask for it at the same time.
    async fn get_shared(&self, path: &str, timeout: Option<std::time::Duration>) -> std::result::Result<Arc<[u8]>, String> {
        let answer = self.asking.lock().unwrap().entry(path.to_string()).or_default().clone();
        let body = answer
            .get_or_init(|| async {
                let body: Result<Arc<[u8]>> = async {
                    let mut req = self.http.get(format!("{}{path}", self.base));
                    if let Some(t) = timeout {
                        req = req.timeout(t);
                    }
                    let resp = self.send(req).await.with_context(|| format!("GET {path}"))?;
                    let status = resp.status();
                    if !status.is_success() {
                        bail!("GET {path} returned {status}");
                    }
                    Ok(resp.bytes().await.with_context(|| format!("GET {path}"))?.to_vec().into())
                }
                .await;
                body.map_err(|e| format!("{e:#}"))
            })
            .await
            .clone();
        let mut asking = self.asking.lock().unwrap();
        if asking.get(path).is_some_and(|a| Arc::ptr_eq(a, &answer)) {
            asking.remove(path);
        }
        body
    }

    /// Unauthenticated; also proves the server is reachable.
    pub async fn public_info(&self) -> Result<PublicInfo> {
        self.get("/System/Info/Public").await
    }

    /// Needs a valid API key (401 otherwise), unlike `public_info`: this is the connection check.
    pub async fn system_info(&self) -> Result<PublicInfo> {
        self.get("/System/Info").await
    }

    pub async fn sessions(&self) -> Result<Vec<Session>> {
        self.get("/Sessions").await
    }

    pub async fn devices(&self) -> Result<Vec<Device>> {
        let page: ItemsPage<Device> = self.get("/Devices").await?;
        Ok(page.items)
    }

    pub async fn virtual_folders(&self) -> Result<Vec<VirtualFolder>> {
        self.get("/Library/VirtualFolders").await
    }

    /// The server's path substitutions (from, to): its API shows every item's path with them applied (`/Volume1/` shown
    /// as `/data/`, say), while its libraries' locations and the paths it is told about stay unsubstituted.
    pub async fn path_substitutions(&self) -> Result<Vec<(String, String)>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Substitution {
            from: String,
            to: String,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Config {
            #[serde(default)]
            path_substitutions: Vec<Substitution>,
        }
        let c: Config = self.get("/System/Configuration").await?;
        Ok(c.path_substitutions.into_iter().filter(|s| !s.to.is_empty()).map(|s| (s.from, s.to)).collect())
    }

    /// Have one library look for new and gone folders now, as its Scan does (only that library). Measured
    /// 2026-10-13: a new album folder in Music was found 10 s after this, where a "file changed" notice alone waits for
    /// the library monitor (about 60 s) and does not look for new folders in a library it does not watch.
    pub async fn refresh_library(&self, library_id: &str) -> Result<()> {
        let path = format!(
            "/Items/{library_id}/Refresh?Recursive=true&MetadataRefreshMode=Default&ImageRefreshMode=Default&ReplaceAllMetadata=false&ReplaceAllImages=false"
        );
        let url = format!("{}{path}", self.base);
        let status = self.send(self.http.post(&url)).await.with_context(|| format!("POST {path}"))?.status();
        if !status.is_success() {
            bail!("POST {path} returned {status}");
        }
        Ok(())
    }

    /// The whole-server library scan ("Scan Media Library"): its progress (percent) while it runs, and when it last
    /// finished (epoch seconds).
    pub async fn library_scan(&self) -> Result<crate::source::LibraryScan> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Task {
            key: Option<String>,
            state: Option<String>,
            current_progress_percentage: Option<f64>,
            last_execution_result: Option<Result_>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Result_ {
            end_time_utc: Option<String>,
        }
        let tasks: Vec<Task> = self.get("/ScheduledTasks").await?;
        let scan = tasks.into_iter().find(|t| t.key.as_deref() == Some("RefreshLibrary"));
        Ok(match scan {
            None => crate::source::LibraryScan::default(),
            Some(t) => crate::source::LibraryScan {
                running: (t.state.as_deref() == Some("Running")).then(|| t.current_progress_percentage.unwrap_or(0.0)),
                last_end: t.last_execution_result.and_then(|r| r.end_time_utc).as_deref().and_then(crate::nowplaying::parse_iso_epoch),
            },
        })
    }

    /// An administrator's id. Libraries are per-user views in Jellyfin's API, so asking which library an item is in
    /// needs a user; an administrator sees every library. Observed on 10.11: without a user, an item's ancestors are
    /// the physical folders (`Folder`, `AggregateFolder`), not its library; and `ParentId` is ignored once `Ids` is
    /// given, so membership cannot be asked per library either.
    pub async fn admin_user(&self) -> Result<String> {
        let users: Vec<UserLite> = self.get("/Users").await?;
        users
            .into_iter()
            .find(|u| u.policy.as_ref().and_then(|p| p.is_administrator) == Some(true))
            .map(|u| u.id)
            .context("Jellyfin has no administrator account")
    }

    /// Every account: (id, name), for choosing whose playlists Cratarr keeps (decision 0012).
    pub async fn users(&self) -> Result<Vec<(String, String)>> {
        let users: Vec<UserName> = self.get("/Users").await?;
        Ok(users.into_iter().map(|u| (u.id, u.name)).collect())
    }

    /// A user's playlists with the stamp that changes when one is saved. Includes `.m3u` playlists imported from a
    /// library (their path is a file in it); the user's own have a path in Jellyfin's data folder.
    pub async fn user_playlists(&self, user: &str) -> Result<Vec<UserPlaylist>> {
        let page: ItemsPage<UserPlaylist> =
            self.get(&format!("/Items?IncludeItemTypes=Playlist&Recursive=true&UserId={user}&Fields=Etag,Path")).await?;
        Ok(page.items)
    }

    /// Every playlist `user` sees (imported `.m3u` ones and their own) with whether it is their favourite: (id, name, on).
    pub async fn playlist_favorites(&self, user: &str) -> Result<Vec<(String, String, bool)>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Item {
            id: String,
            name: String,
            user_data: Option<Fav>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Fav {
            is_favorite: Option<bool>,
        }
        anyhow::ensure!(crate::covers::valid_item_id(user), "not a user id");
        let page: ItemsPage<Item> =
            self.get(&format!("/Items?IncludeItemTypes=Playlist&Recursive=true&UserId={user}&EnableUserData=true")).await?;
        Ok(page.items.into_iter().map(|i| (i.id, i.name, i.user_data.and_then(|u| u.is_favorite).unwrap_or(false))).collect())
    }

    /// A playlist's track ids in order. Jellyfin answers this only with a user (observed on 10.11: 400 without).
    pub async fn playlist_entries(&self, user: &str, playlist: &str) -> Result<Vec<String>> {
        let page: ItemsPage<IdOnly> = self.get_long(&format!("/Playlists/{playlist}/Items?UserId={user}{PLAYLIST_ITEMS_LIGHT}")).await?;
        Ok(page.items.into_iter().map(|i| i.id).collect())
    }

    /// A new playlist owned by `user`, private (`IsPublic: false`), with these tracks. Returns its id. The ids travel
    /// in the body, so any length fits.
    pub async fn create_playlist(&self, user: &str, name: &str, ids: &[String]) -> Result<String> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Created {
            id: String,
        }
        let body = serde_json::json!({ "Name": name, "Ids": ids, "UserId": user, "MediaType": "Audio", "IsPublic": false });
        let created: Created = self.send_json(reqwest::Method::POST, "/Playlists", &body).await?.json().await.context("POST /Playlists: unexpected answer")?;
        Ok(created.id)
    }

    /// A playlist's entries: (track id, entry id). An entry id names one occurrence, for removing it.
    pub async fn playlist_entry_ids(&self, user: &str, playlist: &str) -> Result<Vec<(String, String)>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Entry {
            id: String,
            playlist_item_id: Option<String>,
        }
        let page: ItemsPage<Entry> = self.get_long(&format!("/Playlists/{playlist}/Items?UserId={user}{PLAYLIST_ITEMS_LIGHT}")).await?;
        Ok(page.items.into_iter().map(|e| (e.playlist_item_id.clone().unwrap_or_else(|| e.id.clone()), e.id)).map(|(entry, id)| (id, entry)).collect())
    }

    /// Append tracks, in order. The ids travel in the query string, so they go in batches (see `PLAYLIST_BATCH`).
    pub async fn add_to_playlist(&self, user: &str, playlist: &str, ids: &[String]) -> Result<()> {
        for batch in ids.chunks(PLAYLIST_BATCH) {
            self.send_query(reqwest::Method::POST, &format!("/Playlists/{playlist}/Items"), &[("ids", batch.join(",")), ("userId", user.into())]).await?;
        }
        Ok(())
    }

    /// Remove entries (by entry id), in batches.
    pub async fn remove_from_playlist(&self, user: &str, playlist: &str, entries: &[String]) -> Result<()> {
        for batch in entries.chunks(PLAYLIST_BATCH) {
            self.send_query(reqwest::Method::DELETE, &format!("/Playlists/{playlist}/Items"), &[("entryIds", batch.join(",")), ("userId", user.into())])
                .await?;
        }
        Ok(())
    }

    async fn send_query(&self, method: reqwest::Method, path: &str, query: &[(&str, String)]) -> Result<()> {
        let status =
            self.send(self.http.request(method.clone(), format!("{}{path}", self.base)).query(query)).await.with_context(|| format!("{method} {path}"))?.status();
        if !status.is_success() {
            bail!("{method} {path} returned {status}");
        }
        Ok(())
    }

    /// Delete an item (a playlist). Already gone counts as done, so a repeated delete is harmless.
    pub async fn delete_item(&self, id: &str) -> Result<()> {
        let path = format!("/Items/{id}");
        let status = self.send(self.http.delete(format!("{}{path}", self.base))).await.with_context(|| format!("DELETE {path}"))?.status();
        if !status.is_success() && status != reqwest::StatusCode::NOT_FOUND {
            bail!("DELETE {path} returned {status}");
        }
        Ok(())
    }

    async fn send_json(&self, method: reqwest::Method, path: &str, body: &serde_json::Value) -> Result<reqwest::Response> {
        let resp = self.send(self.http.request(method.clone(), format!("{}{path}", self.base)).json(body)).await.with_context(|| format!("{method} {path}"))?;
        let status = resp.status();
        if !status.is_success() {
            bail!("{method} {path} returned {status}");
        }
        Ok(resp)
    }

    /// Whether `user` has marked the item a favourite.
    pub async fn is_favorite(&self, user: &str, item: &str) -> Result<bool> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct UserData {
            is_favorite: Option<bool>,
        }
        let d: UserData = self.get(&format!("/UserItems/{item}/UserData?userId={user}")).await?;
        Ok(d.is_favorite.unwrap_or(false))
    }

    /// Mark the item a favourite of `user`, or not. Saying the state wanted makes it safe to repeat.
    pub async fn set_favorite(&self, user: &str, item: &str, on: bool) -> Result<()> {
        let method = if on { reqwest::Method::POST } else { reqwest::Method::DELETE };
        self.send_query(method, &format!("/UserFavoriteItems/{item}"), &[("userId", user.into())]).await
    }

    /// The id of the library (collection folder) an item is in, if any, as `user` sees it (see `admin_user`).
    pub async fn library_of(&self, item_id: &str, user: &str) -> Result<Option<String>> {
        let up: Vec<Ancestor> = self.get(&format!("/Items/{item_id}/Ancestors?userId={user}")).await?;
        Ok(up.into_iter().find(|a| a.kind.as_deref() == Some("CollectionFolder")).map(|a| a.id))
    }

    /// A page of a library's tracks (id, path), by sort name so pages are stable; `since` = only tracks Jellyfin saved
    /// after then. Measured on 10.11 with ~24,000 tracks: 500 per page in about 5 s.
    pub async fn track_page(&self, library_id: &str, since: Option<i64>, start: usize, limit: usize) -> Result<(Vec<(String, String)>, usize)> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Row {
            id: String,
            path: Option<String>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Page {
            items: Vec<Row>,
            total_record_count: Option<usize>,
        }
        let mut q = format!(
            "/Items?ParentId={library_id}&IncludeItemTypes=Audio&Recursive=true&Fields=Path&SortBy=SortName&EnableImages=false&EnableUserData=false&StartIndex={start}&Limit={limit}"
        );
        if let Some(t) = since {
            q.push_str(&format!("&MinDateLastSaved={}", iso(t)));
        }
        let page: Page = self.get_long(&q).await?;
        let total = page.total_record_count.unwrap_or(start + page.items.len());
        Ok((page.items.into_iter().filter_map(|r| r.path.map(|p| (r.id, p))).collect(), total))
    }

    /// Paths of up to `n` tracks in a library.
    pub async fn sample_paths(&self, library_id: &str, n: usize) -> Result<Vec<String>> {
        let page: ItemsPage<PathOnly> = self
            .get(&format!("/Items?ParentId={library_id}&IncludeItemTypes=Audio&Recursive=true&Limit={n}&Fields=Path"))
            .await?;
        Ok(page.items.into_iter().filter_map(|i| i.path).collect())
    }

    /// A library's own folders as Jellyfin reports track paths: the parents of its top items (albums or artist
    /// folders, observed on 10.11). A library with several folders has a parent for each, as far as its first `n`
    /// items reach.
    pub async fn library_roots(&self, library_id: &str, n: usize) -> Result<Vec<String>> {
        let page: ItemsPage<PathOnly> = self.get(&format!("/Items?ParentId={library_id}&Limit={n}&Fields=Path")).await?;
        let mut roots: Vec<String> = Vec::new();
        for path in page.items.into_iter().filter_map(|i| i.path) {
            let path = path.replace('\\', "/");
            if let Some((parent, _)) = path.trim_end_matches('/').rsplit_once('/') {
                if !parent.is_empty() && !roots.iter().any(|r| r == parent) {
                    roots.push(parent.to_string());
                }
            }
        }
        Ok(roots)
    }

    /// Albums in a library, newest first, without their tracks (one cheap request, under a second for a hundred
    /// albums), each with the stamp that changes when Jellyfin saves it again.
    pub async fn album_heads(&self, library_id: &str, limit: usize) -> Result<Vec<(crate::source::Album, Option<String>)>> {
        // In pages, one after the other: a whole Music library at once (2,231 albums) took Jellyfin 25 s on 2026-10-15,
        // past the 20 s a request may take, while a page of 250 comes back in about 3 s. Newest first, then by name, so
        // the pages neither overlap nor skip an album added the same moment.
        const PAGE: usize = 250;
        let mut heads = Vec::new();
        while heads.len() < limit {
            let want = PAGE.min(limit - heads.len());
            let page: ItemsPage<ListedAlbumItem> = self
                .get_long(&format!(
                    "/Items?ParentId={library_id}&IncludeItemTypes=MusicAlbum&Recursive=true&SortBy=DateCreated,SortName&SortOrder=Descending&EnableImages=false&EnableUserData=false&Fields=Path,Tags,DateCreated,DateLastSaved&StartIndex={}&Limit={want}",
                    heads.len()
                ))
                .await?;
            let n = page.items.len();
            heads.extend(page.items.into_iter().map(album_head));
            if n < want {
                break;
            }
        }
        Ok(heads)
    }

    /// Track count, format and lowest bitrate of one album (one request, about a tenth of a second). Reading these
    /// for a whole library at once takes Jellyfin seconds, so they are read per album and only when it changes.
    pub async fn album_summary(&self, album_id: &str) -> Result<AlbumSummary> {
        let page: ItemsPage<ListedAudio> =
            self.get(&format!("/Items?ParentId={album_id}&IncludeItemTypes=Audio&Recursive=true&Fields=MediaSources")).await?;
        Ok(summarize(&page.items))
    }

    /// The tracks of an album, in disc and track order.
    pub async fn tracks_of_album(&self, album_id: &str) -> Result<Vec<AlbumTrackItem>> {
        let page: ItemsPage<AlbumTrackItem> = self
            .get(&format!(
                "/Items?ParentId={album_id}&IncludeItemTypes=Audio&Recursive=true&SortBy=ParentIndexNumber,IndexNumber,SortName&Fields=Path&EnableImages=false"
            ))
            .await?;
        Ok(page.items)
    }

    /// Ask a session to play these items now, replacing what it plays. Finamp honours this (it ignores some other
    /// remote commands, such as next-track).
    pub async fn play_now(&self, session_id: &str, item_ids: &[String]) -> Result<()> {
        self.play_command(session_id, "PlayNow", item_ids, None).await
    }

    /// A playback control for one session: `PlayPause`, `NextTrack` or `PreviousTrack` (Jellyfin's names).
    pub async fn playstate(&self, session_id: &str, command: &str) -> Result<()> {
        let url = format!("{}/Sessions/{session_id}/Playing/{command}", self.base);
        let status = self.send(self.http.post(&url)).await.context("POST /Sessions/{id}/Playing/{command}")?.status();
        if !status.is_success() {
            bail!("the player refused the command ({status})");
        }
        Ok(())
    }

    /// `command`: PlayNow, PlayNext or PlayLast. At most 240 ids fit in one request (decision 0006). `start_ticks`: where
    /// in the first track to start (PlayNow).
    pub async fn play_command(&self, session_id: &str, command: &str, item_ids: &[String], start_ticks: Option<i64>) -> Result<()> {
        let url = format!("{}/Sessions/{session_id}/Playing", self.base);
        let ids = item_ids.join(",");
        let mut query = vec![("playCommand", command.to_string()), ("itemIds", ids)];
        if let Some(t) = start_ticks {
            query.push(("startPositionTicks", t.to_string()));
        }
        let status = self
            .send(self.http.post(&url).query(&query))
            .await
            .context("POST /Sessions/{id}/Playing")?
            .status();
        if !status.is_success() {
            bail!("the player refused to play ({status})");
        }
        Ok(())
    }

    /// An item's primary image (content type, bytes); `None` when it has none. Refused above `max_bytes`.
    pub async fn primary_image(&self, item_id: &str, max_bytes: u64) -> Result<Option<(String, Vec<u8>)>> {
        let url = format!("{}/Items/{item_id}/Images/Primary", self.base);
        let resp = self.send(self.http.get(&url)).await.context("GET primary image")?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let resp = resp.error_for_status()?;
        if resp.content_length().is_some_and(|n| n > max_bytes) {
            bail!("cover larger than {max_bytes} bytes");
        }
        let ct = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .filter(|t| t.starts_with("image/"))
            .map(str::to_string);
        let Some(ct) = ct else { return Ok(None) };
        let bytes = resp.bytes().await?;
        if bytes.len() as u64 > max_bytes {
            bail!("cover larger than {max_bytes} bytes");
        }
        Ok(Some((ct, bytes.to_vec())))
    }

    /// Newest entries first.
    pub async fn activity(&self, limit: u32) -> Result<Vec<ActivityEntry>> {
        let page: ItemsPage<ActivityEntry> = self.get(&format!("/System/ActivityLog/Entries?limit={limit}")).await?;
        Ok(page.items)
    }

    /// What Jellyfin knows about the tracks of the album folder named `folder` (its last path segment): runtime,
    /// bitrate, plays and favourite, keyed by file name. Empty when Jellyfin has no such album. `search` is a
    /// title fragment to find it with; the folder name is what picks the right match.
    pub async fn album_tracks(&self, search: &str, folder: &str) -> Result<std::collections::BTreeMap<String, crate::albums::Known>> {
        let url = format!("{}/Items", self.base);
        let found: ItemsPage<ListedAlbum> = self
            .send(self.http.get(&url).query(&[("IncludeItemTypes", "MusicAlbum"), ("Recursive", "true"), ("Fields", "Path"), ("Limit", "200"), ("SearchTerm", search)]))
            .await
            .context("searching albums")?
            .error_for_status()?
            .json()
            .await?;
        let leaf = |p: &str| p.rsplit(['/', '\\']).next().unwrap_or("").to_string();
        let Some(album) = found.items.iter().find(|a| a.path.as_deref().is_some_and(|p| leaf(p) == folder)) else {
            return Ok(Default::default());
        };
        let tracks: ItemsPage<ListedTrack> = self
            .send(self.http.get(&url).query(&[("ParentId", album.id.as_str()), ("IncludeItemTypes", "Audio"), ("Recursive", "true"), ("Fields", "Path,MediaSources"), ("Limit", "500")]))
            .await
            .context("listing album tracks")?
            .error_for_status()?
            .json()
            .await?;
        let mut out = std::collections::BTreeMap::new();
        for t in tracks.items {
            let Some(path) = t.path.as_deref() else { continue };
            out.insert(
                leaf(path),
                crate::albums::Known {
                    seconds: t.run_time_ticks.map(|x| x as f64 / 10_000_000.0),
                    kbps: t.media_sources.as_ref().and_then(|m| m.first()).and_then(|s| s.bitrate).map(|b| (b / 1000) as u32),
                    plays: t.user_data.as_ref().and_then(|u| u.play_count).unwrap_or(0),
                    favourite: t.user_data.as_ref().and_then(|u| u.is_favorite).unwrap_or(false),
                },
            );
        }
        Ok(out)
    }

    /// One item with the fields shown on the page.
    pub async fn item(&self, id: &str) -> Result<PlayingItem> {
        // The single-item call needs a user; the list call by id does not and returns the same fields.
        let page: ItemsPage<PlayingItem> = self.get(&format!("/Items?Ids={id}&Fields=Path")).await?;
        page.items.into_iter().next().with_context(|| format!("Jellyfin has no item {id}"))
    }

    /// What Jellyfin knows about a track's file, and its play count for `user` (none without a user).
    pub async fn media_details(&self, id: &str, user: Option<&str>) -> Result<MediaDetails> {
        anyhow::ensure!(crate::covers::valid_item_id(id), "not an item id");
        let user = user.filter(|u| crate::covers::valid_item_id(u)).map(|u| format!("&userId={u}")).unwrap_or_default();
        let page: ItemsPage<MediaDetails> = self.get(&format!("/Items?Ids={id}&Fields=MediaStreams{user}")).await?;
        page.items.into_iter().next().with_context(|| format!("Jellyfin has no item {id}"))
    }

    /// Which of these items `user` has as favourites, in one request (ids Jellyfin does not know are left out).
    pub async fn favorites(&self, ids: &[String], user: &str) -> Result<Vec<(String, bool)>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Item {
            id: String,
            user_data: Option<Fav>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Fav {
            is_favorite: Option<bool>,
        }
        anyhow::ensure!(crate::covers::valid_item_id(user), "not a user id");
        let ids: Vec<&str> = ids.iter().map(String::as_str).filter(|i| crate::covers::valid_item_id(i)).collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let page: ItemsPage<Item> = self.get(&format!("/Items?Ids={}&userId={user}&EnableUserData=true", ids.join(","))).await?;
        Ok(page.items.into_iter().map(|i| (i.id, i.user_data.and_then(|u| u.is_favorite).unwrap_or(false))).collect())
    }

    /// The audio items `user` has as favourites (their ids).
    pub async fn favorite_audio(&self, user: &str) -> Result<Vec<String>> {
        self.favorite_items(user, "Audio").await
    }

    /// `user`'s favourite albums (their ids).
    pub async fn favorite_albums(&self, user: &str) -> Result<Vec<String>> {
        self.favorite_items(user, "MusicAlbum").await
    }

    /// `user`'s favourite artists, by name (Jellyfin's artists are the album artists and the tracks' artists both).
    pub async fn favorite_artists(&self, user: &str) -> Result<Vec<String>> {
        anyhow::ensure!(crate::covers::valid_item_id(user), "not a user id");
        let page: ItemsPage<NameOnly> =
            self.get(&format!("/Artists?UserId={user}&Filters=IsFavorite&SortBy=SortName&EnableImages=false&EnableUserData=false&Fields=")).await?;
        Ok(page.items.into_iter().map(|i| i.name).collect())
    }

    async fn favorite_items(&self, user: &str, kind: &str) -> Result<Vec<String>> {
        anyhow::ensure!(crate::covers::valid_item_id(user), "not a user id");
        // In pages, the ids only (no images, user data or fields): a few thousand favourites in one request, with
        // Jellyfin's default fields, did not answer within 90 s on 2026-10-14 (4,786 of them, Jellyfin slow per item),
        // while a page of 500 like this took 15 s; 250 keeps a page well inside the 20 s a request may take.
        const PAGE: usize = 250;
        let mut ids = Vec::new();
        loop {
            let page: ItemsPage<IdOnly> = self
                .get(&format!(
                    "/Items?IncludeItemTypes={kind}&Recursive=true&Filters=IsFavorite&UserId={user}&SortBy=SortName&EnableImages=false&EnableUserData=false&Fields=&StartIndex={}&Limit={PAGE}",
                    ids.len()
                ))
                .await?;
            let n = page.items.len();
            ids.extend(page.items.into_iter().map(|i| i.id));
            if n < PAGE {
                return Ok(ids);
            }
        }
    }

    /// A page of `user`'s audio items that pass `filter` (`IsPlayed` or `IsFavorite`), with their user data, by name so the
    /// pages stay put, and how many pass it (decision 0030, amended 2026-10-15: the import). Jellyfin cannot list "played
    /// or favourite" at once: several filters must all hold. Measured on 10.11 (2026-10-15), an account with 5,204 played
    /// and 4,789 favourite tracks: 250 per page took 9 to 12 s whatever the order or fields, 100 took 4.5 s (Jellyfin
    /// builds each item slowly); an hour later 200 took 4.4 to 5.8 s, and the count alone 0.3 s.
    /// Read with the long wait (`LONG_READ`): a busy Jellyfin took 40 s for a page on 2026-10-15, past the usual 20 s.
    pub async fn user_track_data(&self, user: &str, filter: &str, start: usize, limit: usize) -> Result<(Vec<crate::source::TrackData>, usize)> {
        anyhow::ensure!(crate::covers::valid_item_id(user), "not a user id");
        anyhow::ensure!(matches!(filter, "IsPlayed" | "IsFavorite"), "not a filter");
        let page: Counted<DataItem> = self
            .get_long(&format!(
                "/Items?UserId={user}&Recursive=true&IncludeItemTypes=Audio&Filters={filter}&SortBy=SortName&EnableImages=false&EnableUserData=true&Fields=&StartIndex={start}&Limit={limit}"
            ))
            .await?;
        let total = page.total_record_count.unwrap_or(start + page.items.len());
        Ok((page.items.into_iter().map(track_data).collect(), total))
    }

    /// How many of `user`'s audio items pass `filter` (as `user_track_data`), without listing them.
    pub async fn count_user_tracks(&self, user: &str, filter: &str) -> Result<usize> {
        anyhow::ensure!(crate::covers::valid_item_id(user), "not a user id");
        anyhow::ensure!(matches!(filter, "IsPlayed" | "IsFavorite"), "not a filter");
        let page: Counted<IdOnly> =
            self.get_long(&format!("/Items?UserId={user}&Recursive=true&IncludeItemTypes=Audio&Filters={filter}&EnableUserData=false&Fields=&Limit=0")).await?;
        Ok(page.total_record_count.unwrap_or(0))
    }

    /// Every album and artist `user` made a favourite: albums with their folders (Jellyfin keeps no date for a favourite).
    /// Measured 2026-10-15: 356 albums in two pages of 5 to 8 s, 118 artists in 0.2 s.
    pub async fn favorite_groups(&self, user: &str) -> Result<crate::source::FavoriteGroups> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Item {
            id: String,
            name: Option<String>,
            path: Option<String>,
        }
        anyhow::ensure!(crate::covers::valid_item_id(user), "not a user id");
        const PAGE: usize = 250;
        let mut groups = crate::source::FavoriteGroups::default();
        for kind in ["MusicAlbum", "MusicArtist"] {
            let mut start = 0;
            loop {
                let page: ItemsPage<Item> = self
                    .get_long(&format!(
                        "/Items?UserId={user}&Recursive=true&IncludeItemTypes={kind}&Filters=IsFavorite&SortBy=SortName&EnableImages=false&EnableUserData=false&Fields=Path&StartIndex={start}&Limit={PAGE}"
                    ))
                    .await?;
                let n = page.items.len();
                for i in page.items {
                    if kind == "MusicAlbum" {
                        groups.albums.push((crate::source::AlbumRef { id: i.id, track: None, path: i.path, ..Default::default() }, None));
                    } else if let Some(name) = i.name.filter(|n| !n.trim().is_empty()) {
                        groups.artists.push((crate::source::ArtistRef { id: i.id, name }, None));
                    }
                }
                start += n;
                if n < PAGE {
                    break;
                }
            }
        }
        Ok(groups)
    }

    /// A page of every album with its folder and when Jellyfin added it (`DateCreated`), by name, and how many there are.
    /// Measured 2026-10-15: 250 albums in 5 to 7 s, of 2,738.
    pub async fn albums_added(&self, start: usize, limit: usize) -> Result<(Vec<(crate::source::AlbumRef, i64)>, usize)> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Item {
            id: String,
            path: Option<String>,
            date_created: Option<String>,
        }
        let page: Counted<Item> = self
            .get_long(&format!(
                "/Items?Recursive=true&IncludeItemTypes=MusicAlbum&SortBy=SortName&EnableImages=false&EnableUserData=false&Fields=Path,DateCreated&StartIndex={start}&Limit={limit}"
            ))
            .await?;
        let total = page.total_record_count.unwrap_or(start + page.items.len());
        let items = page
            .items
            .into_iter()
            .filter_map(|i| {
                // Jellyfin writes 0001-01-01 for "never".
                let at = i.date_created.as_deref().and_then(crate::nowplaying::parse_iso_epoch).filter(|t| *t > 0)?;
                Some((crate::source::AlbumRef { id: i.id, track: None, path: i.path, ..Default::default() }, at))
            })
            .collect();
        Ok((items, total))
    }

    /// Count one play of the item for `user`: the play count up by one, last played now, played (decision 0022). Jellyfin's
    /// own "mark played" only makes the count at least one, so the count is read and written back.
    pub async fn count_play(&self, item: &str, user: &str) -> Result<()> {
        let data = self.user_item_data(item, user).await?;
        let count = data.get("PlayCount").and_then(serde_json::Value::as_u64).unwrap_or(0);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        self.set_user_item_data(item, user, &json!({ "PlayCount": count + 1, "LastPlayedDate": iso(now), "Played": true })).await
    }

    /// A user's data on an item: plays, last played, favourite, rating, resume position (Jellyfin 10.9+).
    pub async fn user_item_data(&self, item: &str, user: &str) -> Result<serde_json::Value> {
        anyhow::ensure!(crate::covers::valid_item_id(item) && crate::covers::valid_item_id(user), "not an item or user id");
        self.get(&format!("/UserItems/{item}/UserData?userId={user}")).await
    }

    /// Set a user's data on an item (the fields of `user_data_to_keep`). Safe to repeat.
    pub async fn set_user_item_data(&self, item: &str, user: &str, data: &serde_json::Value) -> Result<()> {
        anyhow::ensure!(crate::covers::valid_item_id(item) && crate::covers::valid_item_id(user), "not an item or user id");
        let path = format!("/UserItems/{item}/UserData?userId={user}");
        let status = self.send(self.http.post(format!("{}{path}", self.base)).json(data)).await.with_context(|| format!("POST {path}"))?.status();
        if !status.is_success() {
            bail!("POST {path} returned {status}");
        }
        Ok(())
    }

    /// Tell Jellyfin that files appeared, changed or disappeared (`kind`: "Created", "Modified" or "Deleted"); paths are
    /// as Jellyfin sees them. This is what makes Jellyfin notice a new or removed `.m3u` file (its Music library
    /// does not watch the folder); it does not make Jellyfin re-read a changed playlist, see `refresh_playlist`.
    pub async fn library_changed(&self, changes: &[(String, &str)]) -> Result<()> {
        let updates: Vec<_> = changes.iter().map(|(p, k)| json!({ "Path": p, "UpdateType": k })).collect();
        let url = format!("{}/Library/Media/Updated", self.base);
        let status = self.send(self.http.post(&url).json(&json!({ "Updates": updates }))).await.context("POST /Library/Media/Updated")?.status();
        if !status.is_success() {
            bail!("POST /Library/Media/Updated returned {status}");
        }
        Ok(())
    }

    /// Ask Jellyfin to re-read one imported playlist after its `.m3u` file changed. `name` is the file name
    /// without `.m3u`, which is also the playlist item's name.
    ///
    /// Verified against a real server (10.11): a "file changed" notice (`/Library/Media/Updated`) does nothing,
    /// the `ValidationOnly` and `Default` refresh modes do nothing, plain `FullRefresh` picks up added entries
    /// but keeps removed ones, and `FullRefresh` with `ReplaceAllMetadata=true` handles both (the entry count
    /// followed the file within seconds). Parameters go in the query string, as the API expects.
    pub async fn refresh_playlist(&self, name: &str) -> Result<()> {
        let listing: ItemsPage<ListedPlaylist> = self
            .get("/Items?IncludeItemTypes=Playlist&Recursive=true&Limit=1000")
            .await?;
        let Some(id) = pick_playlist_id(&listing.items, name) else {
            bail!("Jellyfin has no playlist named {name:?}");
        };
        let url = format!(
            "{}/Items/{id}/Refresh?Recursive=false&ImageRefreshMode=None&MetadataRefreshMode=FullRefresh&ReplaceAllMetadata=true",
            self.base
        );
        let status = self.send(self.http.post(&url)).await.context("POST /Items/{id}/Refresh")?.status();
        if !status.is_success() {
            bail!("refreshing playlist {name:?} returned {status}");
        }
        Ok(())
    }
}

/// A path as the API shows it, back as the server itself has it: the first substitution (from, to) whose `to` it
/// starts with, undone.
pub fn unsubstitute(path: &str, substitutions: &[(String, String)]) -> String {
    substitutions
        .iter()
        .find_map(|(from, to)| path.strip_prefix(to.as_str()).map(|rest| format!("{from}{rest}")))
        .unwrap_or_else(|| path.to_string())
}

/// A listing with its whole size.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Counted<T> {
    items: Vec<T>,
    total_record_count: Option<usize>,
}

/// An audio item with the asking user's data on it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DataItem {
    id: String,
    user_data: Option<ItemUserData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ItemUserData {
    play_count: Option<u32>,
    is_favorite: Option<bool>,
    last_played_date: Option<String>,
    /// Jellyfin's 0 to 10.
    rating: Option<f64>,
}

fn track_data(i: DataItem) -> crate::source::TrackData {
    let d = i.user_data;
    let d = d.as_ref();
    crate::source::TrackData {
        id: i.id,
        plays: d.and_then(|d| d.play_count).unwrap_or(0),
        last_played: d.and_then(|d| d.last_played_date.as_deref()).and_then(crate::nowplaying::parse_iso_epoch).filter(|t| *t > 0),
        favorite: d.and_then(|d| d.is_favorite).unwrap_or(false),
        // Jellyfin keeps no date for a favourite.
        favorite_at: None,
        rating: stars(d.and_then(|d| d.rating)),
        // Linked through the catalogue: no names needed.
        meta: None,
    }
}

/// Jellyfin's 0 to 10 rating as 1 to 5 stars; 0 when none.
fn stars(rating: Option<f64>) -> u8 {
    match rating {
        Some(r) if r > 0.0 => ((r / 2.0).round() as u8).clamp(1, 5),
        _ => 0,
    }
}

fn album_head(a: ListedAlbumItem) -> (crate::source::Album, Option<String>) {
    let album = crate::source::Album {
        added: a.date_created.as_deref().and_then(crate::nowplaying::parse_iso_epoch),
        title: a.name.unwrap_or_default(),
        artist: a.album_artist,
        year: a.production_year,
        path: a.path,
        tags: a.tags,
        library: None,
        artists: Vec::new(),
        release: Default::default(),
        id: a.id,
        tracks: 0,
        format: None,
        bitrate_kbps: None,
        duration_secs: a.run_time_ticks.filter(|t| *t > 0).map(ticks_to_secs),
    };
    (album, a.date_last_saved)
}

fn summarize(tracks: &[ListedAudio]) -> AlbumSummary {
    let formats: std::collections::BTreeSet<String> =
        tracks.iter().filter_map(|t| t.container.as_deref()).filter(|c| !c.is_empty()).map(str::to_uppercase).collect();
    let bitrate_kbps = tracks
        .iter()
        .filter_map(|t| t.media_sources.as_ref().and_then(|m| m.first()).and_then(|s| s.bitrate))
        .map(|b| (b / 1000) as u32)
        .min();
    AlbumSummary {
        tracks: tracks.len() as u32,
        format: (formats.len() == 1).then(|| formats.into_iter().next().unwrap_or_default()),
        bitrate_kbps,
        duration_secs: crate::source::total_secs(tracks.iter().map(|t| t.run_time_ticks.map(ticks_to_secs))),
    }
}

/// The part of an item's user data worth carrying to a moved file's new item: `None` when the user never played,
/// rated or favoured it. Only fields Jellyfin accepts back on update.
pub fn user_data_to_keep(data: &serde_json::Value) -> Option<serde_json::Value> {
    const FIELDS: [&str; 7] = ["PlayCount", "Played", "IsFavorite", "LastPlayedDate", "Rating", "Likes", "PlaybackPositionTicks"];
    let worth = data["PlayCount"].as_i64().unwrap_or(0) > 0
        || data["Played"].as_bool() == Some(true)
        || data["IsFavorite"].as_bool() == Some(true)
        || !data["Rating"].is_null()
        || !data["Likes"].is_null()
        || data["PlaybackPositionTicks"].as_i64().unwrap_or(0) > 0;
    worth.then(|| {
        let mut kept = serde_json::Map::new();
        for f in FIELDS {
            if let Some(v) = data.get(f).filter(|v| !v.is_null()) {
                kept.insert(f.to_string(), v.clone());
            }
        }
        serde_json::Value::Object(kept)
    })
}

/// Two items' kept user data (`user_data_to_keep`) as one: plays added up, played or favourite when either was, the
/// latest play, the higher rating, a like when either liked, and the resume point of the one played last.
pub fn combine_user_data(a: &serde_json::Value, b: &serde_json::Value) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    let count = a["PlayCount"].as_i64().unwrap_or(0) + b["PlayCount"].as_i64().unwrap_or(0);
    if count > 0 {
        out.insert("PlayCount".into(), count.into());
    }
    for f in ["Played", "IsFavorite"] {
        if a[f].as_bool() == Some(true) || b[f].as_bool() == Some(true) {
            out.insert(f.into(), true.into());
        } else if !a[f].is_null() || !b[f].is_null() {
            out.insert(f.into(), false.into());
        }
    }
    // Jellyfin writes its dates in one ISO form, so the later one sorts last.
    let date = |d: &serde_json::Value| d["LastPlayedDate"].as_str().map(str::to_string);
    let later_is_b = date(b) > date(a);
    if let Some(d) = if later_is_b { date(b) } else { date(a) } {
        out.insert("LastPlayedDate".into(), d.into());
    }
    if let Some(r) = [&a["Rating"], &b["Rating"]].iter().filter_map(|r| r.as_f64()).reduce(f64::max) {
        out.insert("Rating".into(), r.into());
    }
    match (a["Likes"].as_bool(), b["Likes"].as_bool()) {
        (Some(true), _) | (_, Some(true)) => {
            out.insert("Likes".into(), true.into());
        }
        (Some(false), _) | (_, Some(false)) => {
            out.insert("Likes".into(), false.into());
        }
        _ => {}
    }
    let resume = if later_is_b { &b["PlaybackPositionTicks"] } else { &a["PlaybackPositionTicks"] };
    if !resume.is_null() {
        out.insert("PlaybackPositionTicks".into(), resume.clone());
    }
    serde_json::Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substituted_paths_go_back_to_the_servers_own() {
        let subs = vec![("/Volume1/".to_string(), "/data/".to_string())];
        assert_eq!(unsubstitute("/data/media/Music/Seiho/ABSTRAKTSEX (2013)", &subs), "/Volume1/media/Music/Seiho/ABSTRAKTSEX (2013)");
        assert_eq!(unsubstitute("/media/Music/x", &subs), "/media/Music/x");
        assert_eq!(unsubstitute("/database/x", &subs), "/database/x");
        assert_eq!(unsubstitute("/data/x", &[]), "/data/x");
    }

    #[test]
    fn two_items_play_data_add_up() {
        let old = json!({"PlayCount": 12, "Played": true, "IsFavorite": true, "LastPlayedDate": "2026-03-01T10:00:00.0000000Z", "Rating": 8.0});
        let new = json!({"PlayCount": 2, "Played": true, "LastPlayedDate": "2026-10-12T21:00:00.0000000Z", "PlaybackPositionTicks": 900});
        assert_eq!(
            combine_user_data(&old, &new),
            json!({"PlayCount": 14, "Played": true, "IsFavorite": true, "LastPlayedDate": "2026-10-12T21:00:00.0000000Z", "Rating": 8.0, "PlaybackPositionTicks": 900})
        );
        assert_eq!(combine_user_data(&old, &json!({})), old, "nothing on one side: the other as it was");
    }

    #[test]
    fn only_user_data_with_something_in_it_is_kept_and_only_settable_fields() {
        let none = json!({"PlayCount": 0, "Played": false, "IsFavorite": false, "Key": "x", "ItemId": "y"});
        assert_eq!(user_data_to_keep(&none), None);
        let played = json!({"PlayCount": 7, "Played": true, "IsFavorite": true, "LastPlayedDate": "2026-10-01T20:00:00Z", "Key": "x", "ItemId": "y"});
        assert_eq!(
            user_data_to_keep(&played),
            Some(json!({"PlayCount": 7, "Played": true, "IsFavorite": true, "LastPlayedDate": "2026-10-01T20:00:00Z"}))
        );
        assert!(user_data_to_keep(&json!({"PlayCount": 0, "IsFavorite": true})).is_some(), "a favourite alone counts");
    }

    #[test]
    fn album_heads_and_summaries_read_what_jellyfin_sends() {
        let a: ListedAlbumItem = serde_json::from_str(
            r#"{"Id":"a","Name":"Twirligig","AlbumArtist":"Jonti","ProductionYear":2011,"DateCreated":"2026-10-12T15:39:29.8863581Z","Path":"/m/Jonti/Twirligig (2011)","Tags":["promote"],"DateLastSaved":"2026-10-12T15:40:00Z"}"#,
        )
        .unwrap();
        let (album, saved) = album_head(a);
        assert_eq!((album.title.as_str(), album.year, album.tags.len(), album.tracks), ("Twirligig", Some(2011), 1, 0));
        assert!(album.added.is_some());
        assert_eq!(saved.as_deref(), Some("2026-10-12T15:40:00Z"));
        let flac: Vec<ListedAudio> = serde_json::from_str(
            r#"[{"Container":"flac","MediaSources":[{"Bitrate":884000}]},{"Container":"flac","MediaSources":[{"Bitrate":901000}]}]"#,
        )
        .unwrap();
        assert_eq!(
            summarize(&flac),
            AlbumSummary { tracks: 2, format: Some("FLAC".into()), bitrate_kbps: Some(884), duration_secs: None },
            "lowest, not average"
        );
        let mixed: Vec<ListedAudio> =
            serde_json::from_str(r#"[{"Container":"mp3","MediaSources":[{"Bitrate":320000}]},{"Container":"flac","MediaSources":[{"Bitrate":192000}]}]"#)
                .unwrap();
        assert_eq!(summarize(&mixed), AlbumSummary { tracks: 2, format: None, bitrate_kbps: Some(192), duration_secs: None });
        assert_eq!(summarize(&[]), AlbumSummary::default());
    }

    #[test]
    fn lengths_come_from_ticks_in_seconds() {
        assert_eq!(ticks_to_secs(2_405_000_000), 240.5);
        let timed: Vec<ListedAudio> = serde_json::from_str(r#"[{"Container":"flac","RunTimeTicks":2405000000},{"Container":"flac","RunTimeTicks":1995000000}]"#).unwrap();
        assert_eq!(summarize(&timed).duration_secs, Some(440.0));
        let partly: Vec<ListedAudio> = serde_json::from_str(r#"[{"Container":"flac","RunTimeTicks":2405000000},{"Container":"flac"}]"#).unwrap();
        assert_eq!(summarize(&partly).duration_secs, None, "one track's length unknown");
        let a: ListedAlbumItem = serde_json::from_str(r#"{"Id":"a","Name":"Twirligig","RunTimeTicks":26405000000}"#).unwrap();
        assert_eq!(album_head(a).0.duration_secs, Some(2640.5));
        let a: ListedAlbumItem = serde_json::from_str(r#"{"Id":"a","Name":"Twirligig"}"#).unwrap();
        assert_eq!(album_head(a).0.duration_secs, None);
    }

    #[test]
    fn an_accounts_track_data_reads_as_jellyfin_sends_it() {
        // As 10.11 answered a played listing on 2026-10-15 (trimmed).
        let page: Counted<DataItem> = serde_json::from_str(
            r#"{"Items":[{"Name":"(OO)","Id":"189177e4bc5e102ade47a58f175bdcd8","Type":"Audio","AlbumId":"203caf071e0344091acdd16de0ebcbee",
                "UserData":{"PlaybackPositionTicks":0,"PlayCount":5,"IsFavorite":false,"LastPlayedDate":"2026-09-16T13:00:33.6700176Z","Played":true,"Key":"k","ItemId":"189177e4bc5e102ade47a58f175bdcd8"}},
               {"Name":"Step","Id":"962f1fb63a0b3d0405159378ae2417cd","UserData":{"PlayCount":0,"IsFavorite":true,"Played":false,"Rating":7.0}},
               {"Name":"bare","Id":"a"}],"TotalRecordCount":5203,"StartIndex":0}"#,
        )
        .unwrap();
        assert_eq!(page.total_record_count, Some(5203));
        let d: Vec<crate::source::TrackData> = page.items.into_iter().map(track_data).collect();
        assert_eq!((d[0].plays, d[0].last_played, d[0].favorite, d[0].rating), (5, Some(1_789_563_633), false, 0));
        assert_eq!((d[1].plays, d[1].last_played, d[1].favorite, d[1].rating), (0, None, true, 4), "7 of 10 is 4 of 5 stars, rounded");
        assert_eq!(d[2], crate::source::TrackData { id: "a".into(), ..Default::default() });
        assert_eq!((stars(Some(10.0)), stars(Some(0.4)), stars(Some(0.0)), stars(None)), (5, 1, 0, 0));
    }

    const SESSIONS: &str = include_str!("../tests/fixtures/sessions.json");

    #[test]
    fn epoch_seconds_become_utc_dates() {
        assert_eq!(iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso(1_791_811_680), "2026-10-12T13:28:00Z");
        assert_eq!(crate::nowplaying::parse_iso_epoch(&iso(951_868_800)), Some(951_868_800));
    }

    #[test]
    fn sessions_fixture_parses() {
        let s: Vec<Session> = serde_json::from_str(SESSIONS).unwrap();
        assert!(!s.is_empty());
    }

    #[test]
    fn now_playing_comes_from_the_audio_session() {
        let s: Vec<Session> = serde_json::from_str(SESSIONS).unwrap();
        let np = pick_now_playing(&s, finamp).expect("fixture has an audio session");
        assert_eq!(np.media_type.as_deref(), Some("Audio"));
        assert!(np.path.is_some());
    }

    fn finamp(s: &Session) -> bool {
        s.client.as_deref().is_some_and(|c| c.eq_ignore_ascii_case("Finamp"))
    }

    fn sess(client: &str, id: &str, media: &str) -> Session {
        Session {
            id: None,
            user_id: None,
            supports_remote_control: None,
            supported_commands: None,
            last_activity_date: None,
            last_playback_check_in: None,
            client: Some(client.into()),
            device_name: None,
            now_playing_item: Some(PlayingItem {
                id: id.into(),
                name: id.into(),
                path: None,
                album: None,
                album_id: None,
                album_artist: None,
                media_type: Some(media.into()),
                run_time_ticks: None,
            }),
            play_state: None,
            now_playing_queue_full_items: Vec::new(),
        }
    }

    #[test]
    fn preferred_client_wins_among_audio_sessions() {
        let s = vec![sess("Web", "a", "Audio"), sess("finamp", "b", "Audio")];
        assert_eq!(pick_now_playing(&s, finamp).unwrap().id, "b");
    }

    #[test]
    fn video_sessions_are_ignored() {
        let s = vec![sess("Finamp", "v", "Video"), sess("Web", "a", "Audio")];
        assert_eq!(pick_now_playing(&s, finamp).unwrap().id, "a");
    }

    #[test]
    fn nothing_playing_gives_none() {
        let s = vec![Session { id: None, user_id: None, supports_remote_control: None, supported_commands: None, last_activity_date: None, last_playback_check_in: None, client: Some("Finamp".into()), device_name: None, now_playing_item: None, play_state: None, now_playing_queue_full_items: Vec::new() }];
        assert!(pick_now_playing(&s, finamp).is_none());
        assert!(pick_now_playing(&[], finamp).is_none());
    }

    #[test]
    fn a_playlist_is_found_by_exact_name() {
        let items: Vec<ListedPlaylist> =
            serde_json::from_str(r#"[{"Id":"a","Name":"Chill"},{"Id":"b","Name":"Chill Out"}]"#).unwrap();
        assert_eq!(pick_playlist_id(&items, "Chill").as_deref(), Some("a"));
        assert_eq!(pick_playlist_id(&items, "chill"), None);
        assert_eq!(pick_playlist_id(&[], "Chill"), None);
    }

    #[test]
    fn a_playing_item_without_an_id_fails_loudly() {
        let bad = r#"[{"Client":"Finamp","NowPlayingItem":{"Name":"No id here"}}]"#;
        assert!(serde_json::from_str::<Vec<Session>>(bad).is_err());
    }

    /// A stand-in Jellyfin that answers every request with `[]` after 50 ms: (requests per path, most at once).
    async fn slow_server() -> (String, Arc<Mutex<(HashMap<String, usize>, usize, usize)>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen: Arc<Mutex<(HashMap<String, usize>, usize, usize)>> = Arc::default();
        let s = seen.clone();
        tokio::spawn(async move {
            loop {
                let (mut conn, _) = listener.accept().await.unwrap();
                let s = s.clone();
                tokio::spawn(async move {
                    let mut head = Vec::new();
                    let mut buf = [0u8; 1024];
                    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                        let n = conn.read(&mut buf).await.unwrap();
                        if n == 0 {
                            return;
                        }
                        head.extend_from_slice(&buf[..n]);
                    }
                    let path = String::from_utf8_lossy(&head).split(' ').nth(1).unwrap_or("").to_string();
                    {
                        let mut s = s.lock().unwrap();
                        *s.0.entry(path).or_default() += 1;
                        s.1 += 1;
                        s.2 = s.2.max(s.1);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    s.lock().unwrap().1 -= 1;
                    let _ = conn.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n[]").await;
                });
            }
        });
        (base, seen)
    }

    #[tokio::test]
    async fn at_most_a_few_requests_are_in_flight() {
        let (base, seen) = slow_server().await;
        let jf = Jellyfin::new(&base, "key").unwrap();
        let mut asks = tokio::task::JoinSet::new();
        for i in 0..12 {
            let jf = jf.clone();
            asks.spawn(async move { jf.get::<Vec<serde_json::Value>>(&format!("/Items/{i}")).await });
        }
        while let Some(r) = asks.join_next().await {
            assert!(r.unwrap().unwrap().is_empty());
        }
        let seen = seen.lock().unwrap();
        assert_eq!(seen.0.len(), 12);
        assert!(seen.2 <= MAX_IN_FLIGHT, "{} at once", seen.2);
    }

    #[tokio::test]
    async fn the_same_get_at_the_same_time_is_asked_once() {
        let (base, seen) = slow_server().await;
        let jf = Jellyfin::new(&base, "key").unwrap();
        let mut asks = tokio::task::JoinSet::new();
        for _ in 0..5 {
            let jf = jf.clone();
            asks.spawn(async move { jf.get::<Vec<serde_json::Value>>("/Sessions").await });
        }
        while let Some(r) = asks.join_next().await {
            assert!(r.unwrap().unwrap().is_empty());
        }
        assert_eq!(seen.lock().unwrap().0["/Sessions"], 1);
        // Once answered, the next ask goes to Jellyfin again.
        jf.get::<Vec<serde_json::Value>>("/Sessions").await.unwrap();
        assert_eq!(seen.lock().unwrap().0["/Sessions"], 2);
        assert!(jf.asking.lock().unwrap().is_empty());
    }
}
