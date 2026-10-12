//! Lidarr, only as far as Incoming needs it: when an album is deleted, stop Lidarr from fetching it again
//! (unmonitor), and undo that. Albums are found by their MusicBrainz release group, which is Lidarr's own album id
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

    async fn set_monitored(&self, ids: &[i64], monitored: bool) -> Result<()> {
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

    /// Stop fetching this album. Returns the albums that WERE monitored (to restore on undo); empty when Lidarr
    /// does not know the album or already left it alone.
    pub async fn unmonitor(&self, release_group: &str) -> Result<Vec<i64>> {
        let was: Vec<i64> = self.albums(release_group).await?.into_iter().filter(|a| a.monitored).map(|a| a.id).collect();
        self.set_monitored(&was, false).await?;
        Ok(was)
    }

    /// Undo `unmonitor`.
    pub async fn monitor(&self, ids: &[i64]) -> Result<()> {
        self.set_monitored(ids, true).await
    }
}
