//! Lidarr, only as far as Incoming and Discover need it: when an album is deleted, stop Lidarr from fetching it again
//! (unmonitor: `monitored` then `set_monitored`, see `ports::Acquisition`), and undo that; an album wanted in Discover
//! (decision 0069), fetched (`want`). Albums are found by their MusicBrainz release group, which is Lidarr's own album id
//! (`foreignAlbumId`). Optional: set in Settings → Connections (LIDARR_URL / LIDARR_API_KEY until saved there).

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::json;

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

/// What Want adds an album with (`settings::LidarrConfig`'s choices); a `None`: as the artists Lidarr has.
#[derive(Debug, Clone, Default)]
pub struct AddWith {
    pub quality: Option<i64>,
    pub metadata: Option<i64>,
    pub root: Option<String>,
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
        let mut headers = reqwest::header::HeaderMap::new();
        let mut v = reqwest::header::HeaderValue::from_str(api_key).context("the API key has characters a header cannot hold")?;
        v.set_sensitive(true);
        headers.insert("X-Api-Key", v);
        let http = reqwest::Client::builder().default_headers(headers).timeout(std::time::Duration::from_secs(20)).build()?;
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

    /// Discover's Want (decision 0069): Lidarr fetches the album into Incoming. One it knows, as Seerr does with
    /// Radarr: left alone when it has it already, monitored when it is not, searched for when it is out (one not out yet
    /// waits for Lidarr's own watch). One it does not know is looked up (by its release group, else by its artist and
    /// name) and added, with its artist where Lidarr has not got them (their other albums not monitored), with `with`'s
    /// profiles and root folder (else as the artists it has), and searched for. What was done, in words.
    pub async fn want(&self, with: &AddWith, release_group: Option<&str>, artist: &str, album: &str) -> Result<String> {
        if let Some(rg) = release_group {
            let known = self.albums(rg).await?;
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
        }
        let term = match release_group {
            Some(rg) => format!("lidarr:{rg}"),
            None => format!("{artist} {album}"),
        };
        let found: Vec<serde_json::Value> = self.get("/api/v1/album/lookup", &[("term", term.as_str())]).await?;
        let want = |v: &serde_json::Value| -> bool {
            release_group.is_some() || {
                let k = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<String>();
                v["title"].as_str().is_some_and(|t| k(t) == k(album)) && v["artist"]["artistName"].as_str().is_some_and(|a| k(a) == k(artist))
            }
        };
        let Some(mut found) = found.into_iter().find(want) else { bail!("Lidarr does not know {album} by {artist}") };
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

    /// A GET of Lidarr's API, its answer read as JSON.
    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T> {
        let resp = self.http.get(format!("{}{path}", self.base)).query(query).send().await.context("asking Lidarr")?;
        if !resp.status().is_success() {
            bail!("Lidarr answered {}", resp.status());
        }
        Ok(resp.json().await.context("Lidarr's answer was not understood")?)
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
