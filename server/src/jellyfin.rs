//! Thin Jellyfin API client. Jellyfin supplies what is playing and is told when a playlist file changed; the
//! playlists themselves are `.m3u` files (see `playlists.rs`). Unknown fields are ignored so harmless additions
//! by Jellyfin never break us, while the fields we rely on are required and fail loudly.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ItemsPage<T> {
    pub items: Vec<T>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Session {
    pub client: Option<String>,
    pub device_name: Option<String>,
    pub now_playing_item: Option<PlayingItem>,
    pub play_state: Option<PlayState>,
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

/// A library as configured in Jellyfin. Its `Locations` are not used: they need not match the paths Jellyfin
/// reports for the tracks in it (observed: `/Volume1/media/Music` vs `/data/media/Music/...`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct VirtualFolder {
    pub name: String,
    pub collection_type: Option<String>,
    pub item_id: String,
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
struct PathOnly {
    path: Option<String>,
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
    pub album_artist: Option<String>,
    pub media_type: Option<String>,
    /// Track length; 10 000 000 ticks per second.
    pub run_time_ticks: Option<i64>,
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

#[derive(Clone)]
pub struct Jellyfin {
    http: reqwest::Client,
    base: String,
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
        Ok(Self { http, base: base.trim_end_matches('/').to_string() })
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base, path);
        let resp = self.http.get(&url).send().await.with_context(|| format!("GET {path}"))?;
        let status = resp.status();
        if !status.is_success() {
            bail!("GET {path} returned {status}");
        }
        resp.json::<T>().await.with_context(|| format!("GET {path}: response did not have the expected shape"))
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

    /// The id of the library (collection folder) an item is in, if any, as `user` sees it (see `admin_user`).
    pub async fn library_of(&self, item_id: &str, user: &str) -> Result<Option<String>> {
        let up: Vec<Ancestor> = self.get(&format!("/Items/{item_id}/Ancestors?userId={user}")).await?;
        Ok(up.into_iter().find(|a| a.kind.as_deref() == Some("CollectionFolder")).map(|a| a.id))
    }

    /// Paths of up to `n` tracks in a library.
    pub async fn sample_paths(&self, library_id: &str, n: usize) -> Result<Vec<String>> {
        let page: ItemsPage<PathOnly> = self
            .get(&format!("/Items?ParentId={library_id}&IncludeItemTypes=Audio&Recursive=true&Limit={n}&Fields=Path"))
            .await?;
        Ok(page.items.into_iter().filter_map(|i| i.path).collect())
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
            .http
            .get(&url)
            .query(&[("IncludeItemTypes", "MusicAlbum"), ("Recursive", "true"), ("Fields", "Path"), ("Limit", "200"), ("SearchTerm", search)])
            .send()
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
            .http
            .get(&url)
            .query(&[("ParentId", album.id.as_str()), ("IncludeItemTypes", "Audio"), ("Recursive", "true"), ("Fields", "Path,MediaSources"), ("Limit", "500")])
            .send()
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

    /// Tell Jellyfin that files appeared, changed or disappeared (`kind`: "Created", "Modified" or "Deleted"); paths are
    /// as Jellyfin sees them. This is what makes Jellyfin notice a new or removed `.m3u` file (its Music library
    /// does not watch the folder); it does not make Jellyfin re-read a changed playlist, see `refresh_playlist`.
    pub async fn library_changed(&self, changes: &[(String, &str)]) -> Result<()> {
        let updates: Vec<_> = changes.iter().map(|(p, k)| json!({ "Path": p, "UpdateType": k })).collect();
        let url = format!("{}/Library/Media/Updated", self.base);
        let status = self.http.post(&url).json(&json!({ "Updates": updates })).send().await.context("POST /Library/Media/Updated")?.status();
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
        let status = self.http.post(&url).send().await.context("POST /Items/{id}/Refresh")?.status();
        if !status.is_success() {
            bail!("refreshing playlist {name:?} returned {status}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SESSIONS: &str = include_str!("../tests/fixtures/sessions.json");

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
            client: Some(client.into()),
            device_name: None,
            now_playing_item: Some(PlayingItem {
                id: id.into(),
                name: id.into(),
                path: None,
                album: None,
                album_artist: None,
                media_type: Some(media.into()),
                run_time_ticks: None,
            }),
            play_state: None,
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
        let s = vec![Session { client: Some("Finamp".into()), device_name: None, now_playing_item: None, play_state: None }];
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
}
