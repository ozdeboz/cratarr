//! Lidarr, only as far as Incoming needs it: when an album is deleted, stop Lidarr from fetching it again
//! (unmonitor), and undo that. Albums are found by their MusicBrainz release group, which is Lidarr's own album id
//! (`foreignAlbumId`). Optional: without LIDARR_URL / LIDARR_API_KEY nothing is asked.

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
    /// From LIDARR_URL and LIDARR_API_KEY; `None` when either is unset.
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("LIDARR_URL").ok().filter(|v| !v.trim().is_empty())?;
        let key = std::env::var("LIDARR_API_KEY").ok().filter(|v| !v.trim().is_empty())?;
        let mut headers = reqwest::header::HeaderMap::new();
        let mut v = reqwest::header::HeaderValue::from_str(&key).ok()?;
        v.set_sensitive(true);
        headers.insert("X-Api-Key", v);
        let http = reqwest::Client::builder().default_headers(headers).timeout(std::time::Duration::from_secs(20)).build().ok()?;
        Some(Self { http, base: url.trim_end_matches('/').to_string() })
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
