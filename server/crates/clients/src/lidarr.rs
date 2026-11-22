//! Lidarr, only as far as Incoming and Discover need it: when an album is deleted, stop Lidarr from fetching it again
//! (unmonitor: `monitored` then `set_monitored`, see `ports::Acquisition`), and undo that; an album wanted in Discover
//! (decision 0069), fetched (`want`); requests' albums, how far they are (`progress`), stopped (`unwant`); what Lidarr
//! is looking for and fetching now (`watching`, for the Requests page, decision 0070). Albums are found by their
//! MusicBrainz release group, which is Lidarr's own album id (`foreignAlbumId`). Optional: set in Settings →
//! Connections (LIDARR_URL / LIDARR_API_KEY until saved there).

use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::json;

/// How much of Lidarr's queue is read: more than it ever holds here.
const QUEUE_READ: usize = 200;

/// How many of one album's history events of one type a read takes: more than an album's imports ever are (T9's fake refuses a
/// page of over 1000).
const HISTORY_PAGE: &str = "500";
/// Lidarr's history event types, as its `eventType` filter takes them: a grab, a track's file imported.
const GRABBED: &str = "1";
const IMPORTED: &str = "3";

/// What Lidarr's history says of a deleted copy's folder (U5, T9: `Lidarr::fail_grab`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Imported {
    /// No import of the album went into it: not Lidarr's copy, as far as its history says.
    Not,
    /// Imports went into it, but no grab was marked: the album's newest import went elsewhere, or no grab of that download is in
    /// the history.
    Unmarked,
    /// The album's newest import went into it, and that download's grab is marked failed.
    Failed,
}

/// A copy's folder as Lidarr's history is matched by (OQ-J): its album folder and that folder's parent (`Artist/Album (Year)`),
/// whatever root either side sees them under; none where it has no parent. Pure.
fn album_place(folder: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = folder.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    match parts.as_slice() {
        [.., artist, album] => Some((artist.to_string(), album.to_string())),
        _ => None,
    }
}

/// Whether an import's `importedPath` (a file) went into `place`: its album folder and parent, in that order, among the folders of
/// its path (a disc's folder below them too). Pure.
fn imported_into(path: &str, place: &(String, String)) -> bool {
    let parts: Vec<&str> = path.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    parts[..parts.len().saturating_sub(1)].windows(2).any(|w| w[0] == place.0 && w[1] == place.1)
}

/// A history read of one album's events of one type, newest first, one page.
fn history_query<'a>(event: &'a str, album: &'a str) -> [(&'a str, &'a str); 6] {
    [("page", "1"), ("pageSize", HISTORY_PAGE), ("sortKey", "date"), ("sortDirection", "descending"), ("eventType", event), ("albumId", album)]
}

/// A history page's events of one album and one kind, as Lidarr names them (`grabbed`, `trackFileImported`), whatever its filters did.
fn events<'a>(page: &'a serde_json::Value, album: i64, kind: &'a str) -> impl Iterator<Item = &'a serde_json::Value> + 'a {
    page["records"].as_array().into_iter().flatten().filter(move |r| r["albumId"].as_i64() == Some(album) && r["eventType"].as_str() == Some(kind))
}

#[derive(Clone)]
pub struct Lidarr {
    http: reqwest::Client,
    base: String,
}

#[derive(Debug, Deserialize)]
struct AlbumRow {
    id: i64,
    monitored: bool,
    #[serde(rename = "releaseDate", default)]
    release_date: Option<String>,
    #[serde(default)]
    statistics: Option<AlbumStats>,
}

#[derive(Debug, Deserialize)]
struct AlbumStats {
    #[serde(rename = "trackFileCount", default)]
    files: u32,
    #[serde(rename = "trackCount", default)]
    tracks: u32,
}

impl AlbumRow {
    /// Any of its tracks already there.
    fn has_files(&self) -> bool {
        self.statistics.as_ref().is_some_and(|s| s.files > 0)
    }

    /// Out already (a date in the past, or none known).
    fn out(&self, now: i64) -> bool {
        self.release_date.as_deref().and_then(crate::nowplaying::parse_iso_epoch).is_none_or(|t| t <= now)
    }
}

/// How far Lidarr is with an album (decision 0070): not known to it, known and not fetched, looked for, grabbed and being
/// downloaded (how far, 0 to 1), downloaded and being imported (RQ01.3), or fetched (tracks there of how many).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Progress {
    Unknown,
    NotWanted,
    Looking,
    Fetching { done: f32 },
    /// Downloaded and being imported, as its queue row's state says (`importPending`, `importing`), never as its bytes do: how far
    /// its download came (0 to 1).
    Importing { done: f32 },
    Fetched { tracks: u32, of: u32 },
    /// Its grab failed, or stalls (its queue row says a warning or an error, or failed): the next fetcher's turn (X1). What Lidarr
    /// says of it, and why where it gives it: its own words, which name paths and its address (W6: administrators' alone).
    Stuck { said: Option<String> },
}

/// One album Lidarr is looking for or fetching (the Requests page's "Lidarr is looking for these", decision 0070).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Watched {
    pub album: String,
    pub artist: String,
    pub release_group: Option<String>,
    pub year: Option<i32>,
    /// Being fetched: how far (0 to 1) and what Lidarr says of it (downloading, importing; a warning or an error with
    /// why, which comes first); else looked for.
    pub done: Option<f32>,
    pub status: Option<String>,
}

/// What Lidarr is looking for (its Wanted, monitored and missing) and what it is fetching now (its queue).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Watching {
    pub fetching: Vec<Watched>,
    pub looking: Vec<Watched>,
    /// Lidarr's Wanted holds more than were read.
    pub more: usize,
}

/// Lidarr's queue, as far as the requests read it.
#[derive(Debug, Deserialize)]
struct QueuePage {
    #[serde(default)]
    records: Vec<QueueRow>,
}

#[derive(Debug, Deserialize)]
struct QueueRow {
    #[serde(rename = "albumId", default)]
    album_id: Option<i64>,
    #[serde(default)]
    size: f64,
    #[serde(default)]
    sizeleft: f64,
    #[serde(rename = "trackedDownloadState", default)]
    state: Option<String>,
    /// `ok`, `warning` or `error`: a download stuck says so here, not in its state.
    #[serde(rename = "trackedDownloadStatus", default)]
    status: Option<String>,
    #[serde(rename = "errorMessage", default)]
    error: Option<String>,
    #[serde(rename = "statusMessages", default)]
    messages: Vec<StatusMessage>,
    #[serde(default)]
    album: Option<serde_json::Value>,
    #[serde(default)]
    artist: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct StatusMessage {
    #[serde(default)]
    messages: Vec<String>,
}

impl QueueRow {
    /// Whether its grab failed or stalls (X1): a warning or an error on it, or a state that says it failed.
    fn stuck(&self) -> bool {
        let said = |s: &Option<String>, what: &[&str]| s.as_deref().is_some_and(|s| what.iter().any(|w| s.to_ascii_lowercase().starts_with(w)));
        said(&self.status, &["warning", "error"]) || said(&self.state, &["failed", "importfailed"])
    }

    /// Whether it is downloaded and being imported (RQ01.3): its state says so (`importPending`, `importing`), whatever its bytes say.
    fn importing(&self) -> bool {
        self.state.as_deref().is_some_and(|s| s.eq_ignore_ascii_case("importpending") || s.eq_ignore_ascii_case("importing"))
    }

    fn done(&self) -> f32 {
        if self.size > 0.0 { ((self.size - self.sizeleft) / self.size).clamp(0.0, 1.0) as f32 } else { 0.0 }
    }

    /// What Lidarr says of it: a warning or an error first, with why when it gives it; else its state.
    fn said(&self) -> Option<String> {
        match self.status.as_deref() {
            Some(s) if !s.is_empty() && !s.eq_ignore_ascii_case("ok") => {
                let given = self.messages.iter().flat_map(|m| m.messages.iter()).next().cloned();
                Some(match self.error.clone().filter(|e| !e.is_empty()).or(given) {
                    Some(why) => format!("{s}: {why}"),
                    None => s.to_string(),
                })
            }
            _ => self.state.clone(),
        }
    }
}

/// An album as Lidarr's lists give it (`album`, with its artist inside or beside it).
fn watched(album: &serde_json::Value, artist: Option<&serde_json::Value>) -> Watched {
    let artist = artist.unwrap_or(&album["artist"]);
    Watched {
        album: album["title"].as_str().unwrap_or_default().to_string(),
        artist: artist["artistName"].as_str().unwrap_or_default().to_string(),
        release_group: album["foreignAlbumId"].as_str().map(str::to_string),
        year: album["releaseDate"].as_str().and_then(|d| d.get(..4)?.parse().ok()),
        done: None,
        status: None,
    }
}

/// What Want adds an album with (`settings::LidarrConfig`'s choices); a `None`: as the artists Lidarr has.
#[derive(Debug, Clone, Default)]
pub struct AddWith {
    pub quality: Option<i64>,
    pub metadata: Option<i64>,
    pub root: Option<String>,
}

/// What became of an album wanted: what was done, in words, and its release group (found by its names where it came with
/// none).
#[derive(Debug, Clone, PartialEq)]
pub struct Handed {
    pub said: String,
    pub release_group: String,
}

/// What to look an album up by, in turn, none twice: as asked, by its main artist, by its name alone. On a real Lidarr
/// (2026-11-06) "Grouch / Grouch In Dub Rare Specimens" found other albums and "Grouch Rare Specimens" that one, and one
/// term answered 503 where the album's name alone found it.
fn terms(artist: &str, main: &str, credited: &[String], album: &str) -> Vec<String> {
    let _ = credited;
    let mut out = vec![format!("{artist} {album}")];
    for term in [format!("{main} {album}"), album.to_string()] {
        if !out.contains(&term) {
            out.push(term);
        }
    }
    out
}

/// The album among `found` with this name, by the artist as credited, else by the main one: a name alone is not enough,
/// as other artists have albums of that name.
fn pick(found: &[serde_json::Value], artist: &str, main: &str, album: &str) -> Option<serde_json::Value> {
    let k = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<String>();
    let by = |name: &str| found.iter().find(|v| v["title"].as_str().is_some_and(|t| k(t) == k(album)) && v["artist"]["artistName"].as_str().is_some_and(|a| k(a) == k(name)));
    by(artist).or_else(|| by(main)).cloned()
}

/// Lidarr's profiles and root folders, for the choices on its page in Connections.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Options {
    pub quality: Vec<(i64, String)>,
    pub metadata: Vec<(i64, String)>,
    pub roots: Vec<String>,
}

impl Lidarr {
    pub fn new(url: &str, api_key: &str) -> Result<Self> {
        Self::waiting(url, api_key, std::time::Duration::from_secs(20))
    }

    /// `new`, each call given up after `wait` (a test's Lidarr that does not answer in time, T9.4).
    pub fn waiting(url: &str, api_key: &str, wait: std::time::Duration) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        let mut v = reqwest::header::HeaderValue::from_str(api_key).context("the API key has characters a header cannot hold")?;
        v.set_sensitive(true);
        headers.insert("X-Api-Key", v);
        // Never a redirect (O1): its key stays on the host given.
        let http = crate::app::reach::lan_client().default_headers(headers).timeout(wait).build()?;
        Ok(Self { http, base: url.trim_end_matches('/').to_string() })
    }

    /// Lidarr's version, which also proves the address and key work (a few seconds at most, for the Settings page).
    pub async fn version(&self) -> Result<String> {
        #[derive(Deserialize)]
        struct SystemStatus {
            version: String,
        }
        let url = format!("{}/api/v1/system/status", self.base);
        let resp = self.http.get(&url).timeout(std::time::Duration::from_secs(5)).send().await.context("cannot reach Lidarr")?;
        match resp.status() {
            s if s.is_success() => Ok(resp.json::<SystemStatus>().await.context("Lidarr's answer was not understood")?.version),
            reqwest::StatusCode::UNAUTHORIZED => bail!("Lidarr refused the API key"),
            s => bail!("Lidarr answered {s}"),
        }
    }

    /// Lidarr's albums for a release group, with whether each is monitored now.
    async fn albums(&self, release_group: &str) -> Result<Vec<AlbumRow>> {
        let url = format!("{}/api/v1/album", self.base);
        let resp = self.http.get(&url).query(&[("foreignAlbumId", release_group)]).send().await.context("asking Lidarr")?;
        if !resp.status().is_success() {
            bail!("Lidarr answered {}", resp.status());
        }
        Ok(resp.json().await?)
    }

    /// Lidarr's albums of a release group that it fetches now (monitored).
    pub async fn monitored(&self, release_group: &str) -> Result<Vec<i64>> {
        Ok(self.albums(release_group).await?.into_iter().filter(|a| a.monitored).map(|a| a.id).collect())
    }

    /// Fetch, or stop fetching, these albums. Safe to repeat.
    pub async fn set_monitored(&self, ids: &[i64], monitored: bool) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let url = format!("{}/api/v1/album/monitor", self.base);
        let status = self.http.put(&url).json(&json!({ "albumIds": ids, "monitored": monitored })).send().await.context("telling Lidarr")?.status();
        if !status.is_success() {
            bail!("Lidarr answered {status}");
        }
        Ok(())
    }

    /// The root folder Lidarr adds an album's artist to when none is chosen in Connections: its artists' (the first
    /// one's, as Want takes), else its first root folder; none when it has neither.
    pub async fn artists_root(&self) -> Result<Option<String>> {
        let artists: Vec<serde_json::Value> = self.get("/api/v1/artist", &[]).await?;
        if let Some(root) = artists.first().and_then(|a| a["rootFolderPath"].as_str()) {
            return Ok(Some(root.to_string()));
        }
        Ok(self.options().await?.roots.into_iter().next())
    }

    /// Lidarr's quality and metadata profiles and its root folders, by id and name.
    pub async fn options(&self) -> Result<Options> {
        #[derive(Deserialize)]
        struct Named {
            id: i64,
            name: String,
        }
        #[derive(Deserialize)]
        struct Root {
            path: String,
        }
        let quality: Vec<Named> = self.get("/api/v1/qualityprofile", &[]).await?;
        let metadata: Vec<Named> = self.get("/api/v1/metadataprofile", &[]).await?;
        let roots: Vec<Root> = self.get("/api/v1/rootfolder", &[]).await?;
        Ok(Options {
            quality: quality.into_iter().map(|x| (x.id, x.name)).collect(),
            metadata: metadata.into_iter().map(|x| (x.id, x.name)).collect(),
            roots: roots.into_iter().map(|r| r.path).collect(),
        })
    }

    /// Discover's Want (decision 0069): Lidarr fetches the album into Incoming. One without a release group is found by
    /// its names first (`by_name`): Lidarr may hold it already, and it refuses to add an album twice.
    pub async fn want(&self, with: &AddWith, release_group: Option<&str>, artist: &str, album: &str) -> Result<Handed> {
        let (release_group, named) = match release_group {
            Some(rg) => (rg.to_string(), None),
            None => {
                let found = self.by_name(artist, album).await?;
                (found["foreignAlbumId"].as_str().context("Lidarr's album had no id")?.to_string(), Some(found))
            }
        };
        let said = self.fetch(with, &release_group, named, artist, album).await?;
        Ok(Handed { said, release_group })
    }

    /// The album Lidarr's lookup finds by these names (`terms`, `pick`). A term that fails is passed over for the next;
    /// that failure is what is told if no term finds it.
    async fn by_name(&self, artist: &str, album: &str) -> Result<serde_json::Value> {
        let main = main_artist(artist);
        let mut failed = None;
        for term in terms(artist, &main, &[], album) {
            match self.get::<Vec<serde_json::Value>>("/api/v1/album/lookup", &[("term", term.as_str())]).await {
                Ok(found) => {
                    if let Some(album) = pick(&found, artist, &main, album) {
                        return Ok(album);
                    }
                }
                Err(e) => failed = Some(e),
            }
        }
        match failed {
            Some(e) => Err(e),
            None => bail!("Lidarr does not know {album} by {artist}"),
        }
    }

    /// One Lidarr knows, as Seerr does with Radarr: left alone when it has it already, monitored when it is not, searched
    /// for when it is out (one not out yet waits for Lidarr's own watch). One it does not know (`named`, else looked up by
    /// its release group) is added, with its artist where Lidarr has not got them (their other albums not monitored), with
    /// `with`'s profiles and root folder (else as the artists it has), and searched for. What was done, in words.
    async fn fetch(&self, with: &AddWith, release_group: &str, named: Option<serde_json::Value>, artist: &str, album: &str) -> Result<String> {
        let known = self.albums(release_group).await?;
        if !known.is_empty() {
            if known.iter().any(AlbumRow::has_files) {
                return Ok("Lidarr has it already".into());
            }
            let off: Vec<i64> = known.iter().filter(|a| !a.monitored).map(|a| a.id).collect();
            self.set_monitored(&off, true).await?;
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
            let out: Vec<i64> = known.iter().filter(|a| a.out(now)).map(|a| a.id).collect();
            if out.is_empty() {
                return Ok("Lidarr will fetch it when it comes out".into());
            }
            self.search(&out).await?;
            return Ok("Lidarr is looking for it".into());
        }
        let mut found = match named {
            Some(found) => found,
            None => {
                let term = format!("lidarr:{release_group}");
                let found: Vec<serde_json::Value> = self.get("/api/v1/album/lookup", &[("term", term.as_str())]).await?;
                found.into_iter().next().with_context(|| format!("Lidarr does not know {album} by {artist}"))?
            }
        };
        // As chosen on its page in Connections; what is not chosen as the artists it has are kept (the first one's
        // profiles and root folder), else Lidarr's first of each.
        let artists: Vec<serde_json::Value> = self.get("/api/v1/artist", &[]).await?;
        let (quality, metadata, root) = match artists.first() {
            Some(a) => (a["qualityProfileId"].clone(), a["metadataProfileId"].clone(), a["rootFolderPath"].clone()),
            None => {
                let q: Vec<serde_json::Value> = self.get("/api/v1/qualityprofile", &[]).await?;
                let m: Vec<serde_json::Value> = self.get("/api/v1/metadataprofile", &[]).await?;
                let r: Vec<serde_json::Value> = self.get("/api/v1/rootfolder", &[]).await?;
                let first = |v: &[serde_json::Value], k: &str| v.first().map(|x| x[k].clone()).unwrap_or(serde_json::Value::Null);
                (first(&q, "id"), first(&m, "id"), first(&r, "path"))
            }
        };
        let quality = with.quality.map_or(quality, |q| json!(q));
        let metadata = with.metadata.map_or(metadata, |m| json!(m));
        let root = with.root.clone().map_or(root, |r| json!(r));
        if quality.is_null() || metadata.is_null() || root.is_null() {
            bail!("Lidarr has no quality profile, metadata profile or root folder to add it with");
        }
        found["monitored"] = json!(true);
        found["addOptions"] = json!({ "searchForNewAlbum": true });
        let artist_obj = found["artist"].as_object_mut().context("Lidarr's album had no artist")?;
        artist_obj.insert("qualityProfileId".into(), quality);
        artist_obj.insert("metadataProfileId".into(), metadata);
        artist_obj.insert("rootFolderPath".into(), root);
        artist_obj.insert("monitored".into(), json!(true));
        artist_obj.insert("monitorNewItems".into(), json!("none"));
        artist_obj.insert("addOptions".into(), json!({ "monitor": "none", "searchForMissingAlbums": false }));
        let url = format!("{}/api/v1/album", self.base);
        let resp = self.http.post(&url).json(&found).send().await.context("telling Lidarr")?;
        if !resp.status().is_success() {
            let why = resp.text().await.unwrap_or_default();
            bail!("Lidarr did not add it: {}", why.chars().take(200).collect::<String>());
        }
        Ok("Lidarr added it and is looking for it".into())
    }

    /// How far Lidarr is with each of these release groups' albums (decision 0070): its queue read once for them all,
    /// and only when one is neither unknown to it nor fetched.
    pub async fn progress(&self, release_groups: &[String]) -> Result<HashMap<String, Progress>> {
        let mut out = HashMap::new();
        let mut queue: Option<Vec<QueueRow>> = None;
        for rg in release_groups {
            if out.contains_key(rg) {
                continue;
            }
            let known = self.albums(rg).await?;
            let p = if known.is_empty() {
                Progress::Unknown
            } else if let Some(a) = known.iter().find(|a| a.has_files()) {
                let st = a.statistics.as_ref().map_or((0, 0), |s| (s.files, s.tracks));
                Progress::Fetched { tracks: st.0, of: st.1 }
            } else {
                if queue.is_none() {
                    queue = Some(self.queue(QUEUE_READ).await?);
                }
                let ids: Vec<i64> = known.iter().map(|a| a.id).collect();
                match queue.iter().flatten().find(|q| q.album_id.is_some_and(|id| ids.contains(&id))) {
                    Some(q) if q.stuck() => Progress::Stuck { said: q.said() },
                    Some(q) if q.importing() => Progress::Importing { done: q.done() },
                    Some(q) => Progress::Fetching { done: q.done() },
                    None if known.iter().any(|a| a.monitored) => Progress::Looking,
                    None => Progress::NotWanted,
                }
            };
            out.insert(rg.clone(), p);
        }
        Ok(out)
    }

    /// A request taken back after Lidarr was handed it (decision 0070): its album no longer looked for. The artist and
    /// what it has already are left as they are.
    pub async fn unwant(&self, release_group: &str) -> Result<()> {
        let ids = self.monitored(release_group).await?;
        self.set_monitored(&ids, false).await
    }

    /// What Lidarr fetches now, and the first `limit` of what it is looking for (decision 0070).
    pub async fn watching(&self, limit: usize) -> Result<Watching> {
        let fetching = self
            .queue(QUEUE_READ)
            .await?
            .into_iter()
            .filter_map(|q| {
                let mut w = watched(q.album.as_ref()?, q.artist.as_ref());
                w.done = Some(q.done());
                w.status = q.said();
                Some(w)
            })
            .collect::<Vec<_>>();
        #[derive(Deserialize)]
        struct Missing {
            #[serde(default)]
            records: Vec<serde_json::Value>,
            #[serde(rename = "totalRecords", default)]
            total: usize,
        }
        let size = limit.to_string();
        let missing: Missing = self
            .get(
                "/api/v1/wanted/missing",
                &[("page", "1"), ("pageSize", size.as_str()), ("includeArtist", "true"), ("monitored", "true"), ("sortKey", "releaseDate"), ("sortDirection", "descending")],
            )
            .await?;
        let busy: Vec<Option<String>> = fetching.iter().map(|w| w.release_group.clone()).collect();
        let looking: Vec<Watched> =
            missing.records.iter().map(|a| watched(a, None)).filter(|w| w.release_group.is_none() || !busy.contains(&w.release_group)).collect();
        Ok(Watching { more: missing.total.saturating_sub(missing.records.len()), fetching, looking })
    }

    /// Lidarr's queue, with each entry's album and artist.
    async fn queue(&self, size: usize) -> Result<Vec<QueueRow>> {
        let size = size.to_string();
        let page: QueuePage = self.get("/api/v1/queue", &[("page", "1"), ("pageSize", size.as_str()), ("includeAlbum", "true"), ("includeArtist", "true")]).await?;
        Ok(page.records)
    }

    /// A GET of Lidarr's API, its answer read as JSON.
    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T> {
        let resp = self.http.get(format!("{}{path}", self.base)).query(query).send().await.context("asking Lidarr")?;
        if !resp.status().is_success() {
            bail!("Lidarr answered {}", resp.status());
        }
        Ok(resp.json().await.context("Lidarr's answer was not understood")?)
    }

    /// The folders Lidarr imported tracks into lately (its history's imports, newest first): each one's own name, once
    /// (the intake card's "where from", 0.14).
    pub async fn imported_folders(&self) -> Result<Vec<String>> {
        let v: serde_json::Value = self
            .get("/api/v1/history", &[("page", "1"), ("pageSize", "500"), ("sortKey", "date"), ("sortDirection", "descending"), ("eventType", "3")])
            .await?;
        let mut out: Vec<String> = Vec::new();
        for r in v["records"].as_array().into_iter().flatten() {
            let Some(path) = r["data"]["importedPath"].as_str() else { continue };
            let folder = path.replace('\\', "/").rsplit('/').nth(1).unwrap_or_default().to_string();
            if !folder.is_empty() && !out.contains(&folder) {
                out.push(folder);
            }
        }
        Ok(out)
    }

    /// A deleted copy lying in `folder` as Lidarr's history has it (U5, T9): its album folder and parent (`Artist/Album (Year)`)
    /// matched to each import's `importedPath`, whatever root either side sees them under (OQ-J). The imports (event type 3) of the
    /// release group's albums are read, one read an album, each kept to its album whatever the server did; where the newest of them
    /// all went into that folder, the grab (event type 1) of that import's download (`downloadId`) is marked failed: Lidarr
    /// blocklists that release and, with "Redownload failed" on (its default), looks for another. Never a grab whose import is not
    /// this copy, nor another download of the album (one under way for someone else).
    pub async fn fail_grab(&self, release_group: &str, folder: &str) -> Result<Imported> {
        let Some(place) = album_place(folder) else { return Ok(Imported::Not) };
        // The newest import of them all: its date, album and download, and whether it went into the folder.
        let mut newest: Option<(String, i64, Option<String>, bool)> = None;
        let mut there = false;
        for a in self.albums(release_group).await? {
            let album = a.id.to_string();
            let page: serde_json::Value = self.get("/api/v1/history", &history_query(IMPORTED, &album)).await?;
            for r in events(&page, a.id, "trackFileImported") {
                let (Some(date), Some(path)) = (r["date"].as_str(), r["data"]["importedPath"].as_str()) else { continue };
                let into = imported_into(path, &place);
                there |= into;
                if newest.as_ref().is_none_or(|(d, ..)| date > d.as_str()) {
                    newest = Some((date.to_string(), a.id, r["downloadId"].as_str().map(str::to_string), into));
                }
            }
        }
        if !there {
            return Ok(Imported::Not);
        }
        let Some((_, album, Some(download), true)) = newest else { return Ok(Imported::Unmarked) };
        let id = album.to_string();
        let page: serde_json::Value = self.get("/api/v1/history", &history_query(GRABBED, &id)).await?;
        let grab = events(&page, album, "grabbed").filter(|r| r["downloadId"].as_str() == Some(download.as_str())).filter_map(|r| Some((r["date"].as_str()?, r["id"].as_i64()?))).max();
        let Some((_, grab)) = grab else { return Ok(Imported::Unmarked) };
        let url = format!("{}/api/v1/history/failed/{grab}", self.base);
        let status = self.http.post(&url).send().await.context("telling Lidarr")?.status();
        if !status.is_success() {
            bail!("Lidarr answered {status}");
        }
        Ok(Imported::Failed)
    }

    /// Lidarr looks for these albums now.
    async fn search(&self, ids: &[i64]) -> Result<()> {
        let url = format!("{}/api/v1/command", self.base);
        let status = self.http.post(&url).json(&json!({ "name": "AlbumSearch", "albumIds": ids })).send().await.context("telling Lidarr")?.status();
        if !status.is_success() {
            bail!("Lidarr answered {status}");
        }
        Ok(())
    }
}

/// The first artist credited ("Air feat. Beck", "Air & Beck", "Air, Beck", "Air / Beck" → "Air").
pub fn main_artist(artist: &str) -> String {
    // ASCII only: a letter that grows when lowered would move the cut off the artist's own bytes.
    let lower = artist.to_ascii_lowercase();
    let cut = [" feat.", " feat ", " ft.", " featuring ", " & ", " and ", ", ", " / ", " x ", " vs. ", " vs ", " with "]
        .iter()
        .filter_map(|s| lower.find(s))
        .min()
        .unwrap_or(artist.len());
    artist[..cut].trim().to_string()
}

/// A strict Lidarr for the tests (T9's): the API key checked on every call (401 without it); release group `GROUP`'s one album, 7
/// (404 for any other group); that album's history alone (400 for another album or none, or a page of over 1000), each event type
/// as asked (1 its grabs, 3 its track imports, each with its `downloadId` and `data.importedPath`); a grab marked failed answered
/// only for one of its grabs (404 else); an album monitored and a search taken; 404 for anything else. Every write is written down,
/// whatever is answered (`Seen::writes`), and every call it did not expect (`Seen::refused`), for the test to fail on.
#[cfg(any(test, feature = "test-support"))]
pub mod fake {
    use axum::Json;
    use axum::body::Bytes;
    use axum::http::{HeaderMap, Method, StatusCode, Uri};
    use axum::response::{IntoResponse, Response};
    use serde_json::{Value, json};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// The release group Lidarr is told by.
    pub const GROUP: &str = "rg-1";
    /// Its one album of the group.
    const ALBUM: i64 = 7;
    /// Its grabs of it (T9): 52, D2's, on 12 November, still downloading for another asker; 50, D1's, on the 10th. Newest first.
    const GRABS: [(i64, &str, &str); 2] = [(52, "D2", "2026-11-12T10:00:00Z"), (50, "D1", "2026-11-10T10:00:00Z")];
    const FAILED: &str = "/api/v1/history/failed/";

    /// How it answers: as Lidarr does; 503 to everything (away); or not in time.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Mode {
        Answers,
        Away,
        Slow,
    }

    /// What it was asked, for the test to read after, and how it answers now (`answers`).
    #[derive(Clone, Default)]
    pub struct Seen {
        writes: Arc<Mutex<Vec<String>>>,
        reads: Arc<Mutex<Vec<String>>>,
        refused: Arc<Mutex<Vec<String>>>,
        mode: Arc<Mutex<Option<Mode>>>,
    }

    impl Seen {
        /// From now on it answers as `mode` says (RF3: away at a Delete, back at a later round).
        pub fn answers(&self, mode: Mode) {
            *self.mode.lock().unwrap() = Some(mode);
        }

        /// Each write, in order: "failed 50", "monitor [7] true", "search [7]", else its method and path.
        pub fn writes(&self) -> Vec<String> {
            self.writes.lock().unwrap().clone()
        }

        /// Each history read, by the event type it asked for: "history 3".
        pub fn reads(&self) -> Vec<String> {
            self.reads.lock().unwrap().clone()
        }

        /// Each call it did not expect: no key, another group or album, a page too long, an id not a grab, another path.
        pub fn refused(&self) -> Vec<String> {
            self.refused.lock().unwrap().clone()
        }
    }

    /// Three track imports of download `download` into `folder`, as Lidarr's own container names it, on `day`.
    pub fn imports(download: &str, folder: &str, day: &str) -> Vec<Value> {
        (1..=3)
            .map(|n| {
                json!({ "id": 600 + n, "albumId": ALBUM, "trackId": 100 + n, "eventType": "trackFileImported", "downloadId": download,
                    "date": format!("{day}T12:00:0{n}Z"),
                    "data": { "importedPath": format!("{folder}/0{n} Track {n}.flac"), "droppedPath": format!("/downloads/{download}/0{n} Track {n}.flac") } })
            })
            .collect()
    }

    /// The fake at a local address, answering as `mode` says, `imports` in its history: its address, and what it is asked.
    pub async fn lidarr(mode: Mode, imports: Vec<Value>) -> (String, Seen) {
        let seen = Seen::default();
        seen.answers(mode);
        let (s, imports) = (seen.clone(), Arc::new(imports));
        let app = axum::Router::new().fallback(move |method: Method, uri: Uri, headers: HeaderMap, body: Bytes| {
            let (s, imports) = (s.clone(), imports.clone());
            async move { answer(&s, &imports, &method, &uri, &headers, &body).await }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        crate::run::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (url, seen)
    }

    async fn answer(seen: &Seen, imports: &[Value], method: &Method, uri: &Uri, headers: &HeaderMap, body: &[u8]) -> Response {
        let path = uri.path();
        let q: HashMap<String, String> = serde_urlencoded::from_str(uri.query().unwrap_or_default()).unwrap_or_default();
        let sent: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
        let ids = |v: &Value| v["albumIds"].as_array().map(|a| a.iter().filter_map(Value::as_i64).collect::<Vec<_>>()).unwrap_or_default();
        if *method != Method::GET {
            let what = match (method.as_str(), path) {
                ("POST", p) if p.starts_with(FAILED) => format!("failed {}", &p[FAILED.len()..]),
                ("PUT", "/api/v1/album/monitor") => format!("monitor {:?} {}", ids(&sent), sent["monitored"]),
                ("POST", "/api/v1/command") if sent["name"] == "AlbumSearch" => format!("search {:?}", ids(&sent)),
                _ => format!("{method} {path}"),
            };
            seen.writes.lock().unwrap().push(what);
        }
        let mode = (*seen.mode.lock().unwrap()).unwrap_or(Mode::Answers);
        match mode {
            Mode::Answers => {}
            Mode::Away => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
            Mode::Slow => {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
        }
        let refuse = |status: StatusCode, why: String| {
            seen.refused.lock().unwrap().push(why);
            status.into_response()
        };
        if headers.get("X-Api-Key").and_then(|v| v.to_str().ok()) != Some("key") {
            return refuse(StatusCode::UNAUTHORIZED, format!("no key: {method} {path}"));
        }
        match (method.as_str(), path) {
            ("GET", "/api/v1/album") => match q.get("foreignAlbumId").map(String::as_str) {
                Some(GROUP) => Json(json!([{ "id": ALBUM, "monitored": false, "title": "Both Shores", "foreignAlbumId": GROUP }])).into_response(),
                other => refuse(StatusCode::NOT_FOUND, format!("another group: {other:?}")),
            },
            ("GET", "/api/v1/history") => {
                if q.get("albumId").map(String::as_str) != Some("7") {
                    return refuse(StatusCode::BAD_REQUEST, format!("another album: {:?}", q.get("albumId")));
                }
                let Some(size) = q.get("pageSize").and_then(|s| s.parse::<usize>().ok()).filter(|s| (1..=1000).contains(s)) else {
                    return refuse(StatusCode::BAD_REQUEST, format!("a page of {:?}", q.get("pageSize")));
                };
                let event = q.get("eventType").cloned().unwrap_or_default();
                seen.reads.lock().unwrap().push(format!("history {event}"));
                let grabs = || {
                    GRABS.iter().map(|(id, download, date)| {
                        json!({ "id": id, "albumId": ALBUM, "eventType": "grabbed", "downloadId": download, "date": date, "sourceTitle": "Salt Meridian - Both Shores (2018) [FLAC]", "data": {} })
                    })
                };
                let mut records: Vec<Value> = match event.as_str() {
                    "1" => grabs().collect(),
                    "3" => imports.to_vec(),
                    "" => grabs().chain(imports.iter().cloned()).collect(),
                    _ => Vec::new(),
                };
                records.sort_by(|a, b| b["date"].as_str().cmp(&a["date"].as_str()));
                let total = records.len();
                records.truncate(size);
                Json(json!({ "page": 1, "pageSize": size, "sortKey": "date", "sortDirection": "descending", "totalRecords": total, "records": records })).into_response()
            }
            ("POST", p) if p.starts_with(FAILED) => {
                let id = p[FAILED.len()..].parse::<i64>().ok();
                if GRABS.iter().any(|(grab, ..)| Some(*grab) == id) { Json(json!({})).into_response() } else { refuse(StatusCode::NOT_FOUND, format!("not a grab: {p}")) }
            }
            ("PUT", "/api/v1/album/monitor") => (StatusCode::ACCEPTED, Json(json!([{ "id": ALBUM }]))).into_response(),
            ("POST", "/api/v1/command") => (StatusCode::CREATED, Json(json!({ "id": 1, "name": sent["name"] }))).into_response(),
            _ => refuse(StatusCode::NOT_FOUND, format!("{method} {path}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Query;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::routing::{get, post, put};
    use axum::{Json, Router};
    use serde_json::Value;
    use std::future::ready;
    use std::sync::{Arc, Mutex};

    type Calls = Arc<Mutex<Vec<String>>>;

    fn album(title: &str, artist: &str, id: &str) -> Value {
        json!({ "title": title, "artist": { "artistName": artist }, "foreignAlbumId": id })
    }

    /// Lidarr at a local address: `look` answers its lookup (none: 503), `known` its albums of a release group, and each
    /// call is written down.
    async fn lidarr_at(look: fn(&str) -> Option<Value>, known: Value) -> (Lidarr, Calls) {
        let calls = Calls::default();
        let (a, b, c, d) = (calls.clone(), calls.clone(), calls.clone(), calls.clone());
        let lookup = get(move |Query(q): Query<HashMap<String, String>>| {
            let term = q["term"].clone();
            a.lock().unwrap().push(format!("lookup {term}"));
            ready(look(&term).map_or_else(|| StatusCode::SERVICE_UNAVAILABLE.into_response(), |v| Json(v).into_response()))
        });
        let albums = get(move |Query(q): Query<HashMap<String, String>>| {
            b.lock().unwrap().push(format!("albums {}", q["foreignAlbumId"]));
            ready(Json(known.clone()))
        })
        .post(move || {
            c.lock().unwrap().push("add".to_string());
            ready(Json(json!({})))
        });
        let e = d.clone();
        let monitor = put(move || {
            d.lock().unwrap().push("monitor".to_string());
            ready(Json(json!({})))
        });
        let search = post(move || {
            e.lock().unwrap().push("search".to_string());
            ready(Json(json!({})))
        });
        let artists = get(|| ready(Json(json!([{ "qualityProfileId": 1, "metadataProfileId": 1, "rootFolderPath": "/music" }]))));
        let app = Router::new()
            .route("/api/v1/album/lookup", lookup)
            .route("/api/v1/album/monitor", monitor)
            .route("/api/v1/album", albums)
            .route("/api/v1/command", search)
            .route("/api/v1/artist", artists);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        crate::run::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (Lidarr::new(&url, "key").unwrap(), calls)
    }

    #[test]
    fn an_album_is_looked_up_as_asked_by_its_main_artist_and_by_its_name_none_twice() {
        assert_eq!(terms("Grouch / Grouch In Dub", "Grouch", &[], "Rare Specimens"), ["Grouch / Grouch In Dub Rare Specimens", "Grouch Rare Specimens", "Rare Specimens"]);
        assert_eq!(terms("Air", "Air", &[], "Moon Safari"), ["Air Moon Safari", "Moon Safari"]);
    }

    /// SCENARIOS-DC3 F54.4 (5), Lidarr's half (T-DC3-32, M2): its terms gain a term per credited artist of the record, by its one name,
    /// after the whole credit and the main artist and before the album's name alone: Deep Harbour, credited Commodo and Alfa Mist,
    /// looks up "Commodo Deep Harbour" and "Alfa Mist Deep Harbour". `main_artist` stays a term, never a credit; a record of one artist
    /// reads as before. Red first: no term for Alfa Mist.
    #[test]
    fn the_terms_try_each_credited_artist() {
        let credited = ["Commodo".to_string(), "Alfa Mist".to_string()];
        assert_eq!(
            terms("Commodo & Alfa Mist", &main_artist("Commodo & Alfa Mist"), &credited, "Deep Harbour"),
            ["Commodo & Alfa Mist Deep Harbour", "Commodo Deep Harbour", "Alfa Mist Deep Harbour", "Deep Harbour"]
        );
        assert_eq!(terms("Air", "Air", &["Air".to_string()], "Moon Safari"), ["Air Moon Safari", "Moon Safari"], "one artist: as before");
    }

    #[test]
    fn the_album_is_its_name_and_its_artist_the_whole_credit_first() {
        let found = [album("At TreePeople", "Saul Williams", "main"), album("At TreePeople", "Someone Else", "other")];
        assert_eq!(pick(&found, "Saul Williams, Carlos Niño & Friends", "Saul Williams", "At TreePeople").unwrap()["foreignAlbumId"], "main");
        let found = [album("Air & Beck", "Air", "main"), album("Air & Beck", "Air & Beck", "whole")];
        assert_eq!(pick(&found, "Air & Beck", "Air", "Air & Beck").unwrap()["foreignAlbumId"], "whole");
        assert!(pick(&[album("Cartouche", "Georges Delerue", "other")], "The Funky Lowlives", "The Funky Lowlives", "Cartouche").is_none());
    }

    #[tokio::test]
    async fn one_lidarr_holds_is_found_by_its_main_artist_and_monitored_not_added_again() {
        fn look(term: &str) -> Option<Value> {
            let other = album("Grouch in Dub", "Grouch", "rg-dub");
            Some(if term == "Grouch Rare Specimens" { json!([other, album("Rare Specimens", "Grouch", "rg-rare")]) } else { json!([other]) })
        }
        let (lidarr, calls) = lidarr_at(look, json!([{ "id": 7, "monitored": false }])).await;
        let handed = lidarr.want(&AddWith::default(), None, "Grouch / Grouch In Dub", "Rare Specimens").await.unwrap();
        assert_eq!(handed, Handed { said: "Lidarr is looking for it".into(), release_group: "rg-rare".into() });
        assert_eq!(*calls.lock().unwrap(), ["lookup Grouch / Grouch In Dub Rare Specimens", "lookup Grouch Rare Specimens", "albums rg-rare", "monitor", "search"]);
    }

    #[tokio::test]
    async fn a_term_that_fails_is_passed_over_and_one_lidarr_lacks_is_added() {
        fn look(term: &str) -> Option<Value> {
            (!term.contains("Ross From Friends")).then(|| json!([album("Love Divide / The Knife", "Ross From Friends", "rg-love")]))
        }
        let (lidarr, calls) = lidarr_at(look, json!([])).await;
        let handed = lidarr.want(&AddWith::default(), None, "Ross From Friends", "Love Divide / The Knife").await.unwrap();
        assert_eq!(handed, Handed { said: "Lidarr added it and is looking for it".into(), release_group: "rg-love".into() });
        assert_eq!(*calls.lock().unwrap(), ["lookup Ross From Friends Love Divide / The Knife", "lookup Love Divide / The Knife", "albums rg-love", "add"]);
    }

    #[tokio::test]
    async fn found_by_no_term_it_says_so_and_a_failure_is_told_as_one() {
        fn another(_: &str) -> Option<Value> {
            Some(json!([album("Cartouche", "Georges Delerue", "rg-other")]))
        }
        let (lidarr, calls) = lidarr_at(another, json!([])).await;
        let e = lidarr.want(&AddWith::default(), None, "The Funky Lowlives", "Cartouche").await.unwrap_err();
        assert_eq!(e.to_string(), "Lidarr does not know Cartouche by The Funky Lowlives");
        assert!(!calls.lock().unwrap().iter().any(|c| c == "add"), "another artist's album of that name is not added");
        let (lidarr, _) = lidarr_at(|_| None, json!([])).await;
        let e = lidarr.want(&AddWith::default(), None, "The Funky Lowlives", "Cartouche").await.unwrap_err();
        assert_eq!(e.to_string(), "Lidarr answered 503 Service Unavailable");
    }

    /// U5, T9 (the strict fake, in place of one that answered whatever it was asked): the grab marked failed is the one whose download
    /// the album's newest import is, and only where that import went into the copy's folder, matched by the album folder and its
    /// parent whatever root Lidarr's container gives them (OQ-J), a disc's folder below them too. An older import there and a newer
    /// one elsewhere mark none; imports only elsewhere (a folder whose name begins alike, another artist's of that name), or none,
    /// say Lidarr never imported there. Without the key, nothing is read or marked.
    #[tokio::test]
    async fn the_grab_of_the_newest_import_into_the_folder_is_marked_failed() {
        const HERE: &str = "Salt Meridian/Both Shores (2018)";
        let lidarrs = |folder: &str| format!("/data/music/{folder}");
        let cases = [
            (fake::imports("D1", &lidarrs(HERE), "2026-11-10"), Imported::Failed, vec!["failed 50"]),
            (fake::imports("D1", &lidarrs("Salt Meridian/Both Shores (2018)/CD 02"), "2026-11-10"), Imported::Failed, vec!["failed 50"]),
            ([fake::imports("D1", &lidarrs(HERE), "2026-11-10"), fake::imports("D2", &lidarrs("Salt Meridian/Both Shores (2018) (Deluxe)"), "2026-11-13")].concat(), Imported::Unmarked, vec![]),
            ([fake::imports("D1", &lidarrs("Salt Meridian/Both Shores (2018) (Deluxe)"), "2026-11-10"), fake::imports("D3", &lidarrs("Other Artist/Both Shores (2018)"), "2026-11-11")].concat(), Imported::Not, vec![]),
            (Vec::new(), Imported::Not, vec![]),
        ];
        for (n, (imports, imported, writes)) in cases.into_iter().enumerate() {
            let (url, seen) = fake::lidarr(fake::Mode::Answers, imports).await;
            assert_eq!(Lidarr::new(&url, "key").unwrap().fail_grab(fake::GROUP, HERE).await.unwrap(), imported, "case {n}");
            assert_eq!(seen.writes(), writes, "case {n}");
            assert!(seen.refused().is_empty(), "case {n}: {:?}", seen.refused());
        }
        let (url, seen) = fake::lidarr(fake::Mode::Answers, fake::imports("D1", &lidarrs(HERE), "2026-11-10")).await;
        assert!(Lidarr::new(&url, "another key").unwrap().fail_grab(fake::GROUP, HERE).await.is_err(), "refused");
        assert!(seen.writes().is_empty() && seen.reads().is_empty());
    }

    #[tokio::test]
    async fn a_release_group_goes_to_lidarr_as_it_is_with_no_search_by_name() {
        fn look(_: &str) -> Option<Value> {
            Some(json!([album("Moon Safari", "Air", "rg-1")]))
        }
        let (lidarr, calls) = lidarr_at(look, json!([])).await;
        let handed = lidarr.want(&AddWith::default(), Some("rg-1"), "Air", "Moon Safari").await.unwrap();
        assert_eq!(handed, Handed { said: "Lidarr added it and is looking for it".into(), release_group: "rg-1".into() });
        assert_eq!(*calls.lock().unwrap(), ["albums rg-1", "lookup lidarr:rg-1", "add"]);
    }

    /// RQ01.3: how far Lidarr is with each album, from its albums and its queue as Lidarr answers them, the queue read once for them
    /// all. G1, monitored and in no queue row: looked for. G2, downloading, 42 of its 100 bytes there: grabbed, 0.42. G3 waiting to
    /// import and G4 importing: importing, as their rows' state says. G5, its row's status a warning: stuck, in Lidarr's own words and
    /// why. G6, its 11 tracks of 11 there: fetched. G7, every byte there but its state still downloading: grabbed, never read as
    /// importing from its bytes.
    #[tokio::test]
    async fn lidarrs_queue_says_grabbed_or_importing() {
        const WHY: &str = "Import failed: /media/music not found on http://192.168.1.20:8686";
        let albums = json!({
            "g1": [{ "id": 1, "monitored": true }],
            "g2": [{ "id": 2, "monitored": true }],
            "g3": [{ "id": 3, "monitored": true }],
            "g4": [{ "id": 4, "monitored": true }],
            "g5": [{ "id": 5, "monitored": true }],
            "g6": [{ "id": 6, "monitored": true, "statistics": { "trackFileCount": 11, "trackCount": 11 } }],
            "g7": [{ "id": 7, "monitored": true }],
        });
        let row = |album: i64, left: f64, state: &str, status: &str| {
            json!({ "albumId": album, "size": 100.0, "sizeleft": left, "trackedDownloadState": state, "trackedDownloadStatus": status })
        };
        let mut stuck = row(5, 0.0, "importPending", "warning");
        stuck["statusMessages"] = json!([{ "title": "Night Ferry", "messages": [WHY] }]);
        let queue = json!({ "records": [row(2, 58.0, "downloading", "ok"), row(3, 0.0, "importPending", "ok"), row(4, 0.0, "importing", "ok"), stuck, row(7, 0.0, "downloading", "ok")] });
        let calls = Calls::default();
        let (a, b) = (calls.clone(), calls.clone());
        let app = Router::new()
            .route(
                "/api/v1/album",
                get(move |Query(q): Query<HashMap<String, String>>| {
                    let group = q["foreignAlbumId"].clone();
                    a.lock().unwrap().push(format!("albums {group}"));
                    ready(Json(albums[group.as_str()].clone()))
                }),
            )
            .route(
                "/api/v1/queue",
                get(move || {
                    b.lock().unwrap().push("queue".to_string());
                    ready(Json(queue.clone()))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        crate::run::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let groups: Vec<String> = (1..=7).map(|n| format!("g{n}")).collect();
        let p = Lidarr::new(&url, "key").unwrap().progress(&groups).await.unwrap();
        assert_eq!(p["g1"], Progress::Looking, "looked for");
        assert!(matches!(p["g2"], Progress::Fetching { done } if (done - 0.42).abs() < 1e-6), "grabbed, 42%: {:?}", p["g2"]);
        assert_eq!((&p["g3"], &p["g4"]), (&Progress::Importing { done: 1.0 }, &Progress::Importing { done: 1.0 }), "importing, by the rows' state");
        assert_eq!(p["g5"], Progress::Stuck { said: Some(format!("warning: {WHY}")) }, "stuck, in Lidarr's own words");
        assert_eq!(p["g6"], Progress::Fetched { tracks: 11, of: 11 }, "imported");
        assert_eq!(p["g7"], Progress::Fetching { done: 1.0 }, "every byte there, still grabbed: importing is read from the state alone");
        assert_eq!(calls.lock().unwrap().iter().filter(|c| *c == "queue").count(), 1, "the queue read once for them all");
    }
}
