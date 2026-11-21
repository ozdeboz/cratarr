//! `track_fetch`'s release, in records: a release by its MusicBrainz id, as MusicBrainz lists its tracks, which records'
//! references read. It asks through Discover's client, so it sits beside it. `track_fetch` re-exports it.

use std::path::PathBuf;

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;

use super::musicbrainz::Asked as _;

/// The file that holds a release's track.
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
pub struct Have {
    pub source: String,
    pub id: String,
}

/// One of the release's tracks, as the page lists them.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseTrack {
    pub disc: u32,
    pub position: u32,
    pub title: String,
    pub artist: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length_ms: Option<i64>,
    pub recording: String,
    pub have: Option<Have>,
    #[serde(skip)]
    pub track_id: String,
    #[serde(skip)]
    pub artist_id: Option<String>,
    #[serde(skip)]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseView {
    pub release: String,
    pub title: String,
    pub artist: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    pub tracks: Vec<ReleaseTrack>,
    /// Its pages elsewhere as MusicBrainz links them (`url-rels`, F14.5): its Bandcamp page among them.
    #[serde(skip)]
    pub links: Vec<String>,
}

/// A release's tracks as MusicBrainz gives them (with its recordings and artist credits).
pub fn release_of(v: &Value) -> ReleaseView {
    let credit = |c: &Value| -> (String, Option<String>) {
        let parts = c.as_array().cloned().unwrap_or_default();
        let name = parts.iter().map(|p| format!("{}{}", p["name"].as_str().unwrap_or_default(), p["joinphrase"].as_str().unwrap_or_default())).collect::<String>();
        (name.trim().to_string(), parts.first().and_then(|p| p["artist"]["id"].as_str()).map(str::to_string))
    };
    let mut tracks = Vec::new();
    for (d, m) in v["media"].as_array().into_iter().flatten().enumerate() {
        let disc = m["position"].as_u64().map_or(d as u32 + 1, |p| p as u32);
        for (i, t) in m["tracks"].as_array().into_iter().flatten().enumerate() {
            let (artist, artist_id) = credit(&t["artist-credit"]);
            tracks.push(ReleaseTrack {
                disc,
                position: t["position"].as_u64().map_or(i as u32 + 1, |p| p as u32),
                title: t["title"].as_str().unwrap_or_default().to_string(),
                artist,
                length_ms: t["length"].as_i64().or_else(|| t["recording"]["length"].as_i64()),
                recording: t["recording"]["id"].as_str().unwrap_or_default().to_string(),
                have: None,
                track_id: t["id"].as_str().unwrap_or_default().to_string(),
                artist_id,
                path: None,
            });
        }
    }
    ReleaseView {
        release: v["id"].as_str().unwrap_or_default().to_string(),
        title: v["title"].as_str().unwrap_or_default().to_string(),
        artist: credit(&v["artist-credit"]).0,
        year: v["date"].as_str().and_then(|d| d.get(..4)).and_then(|y| y.parse().ok()),
        tracks,
        links: v["relations"].as_array().into_iter().flatten().filter_map(|r| r["url"]["resource"].as_str().map(str::to_string)).collect(),
    }
}

/// A release by its MusicBrainz id, as MusicBrainz lists its tracks (none held: a record not in the library yet).
pub async fn release_by_id(id: &str) -> Result<ReleaseView> {
    let http = super::discover::http()?;
    let v: Value = http
        .get(format!("https://musicbrainz.org/ws/2/release/{id}?inc=recordings+artist-credits+url-rels&fmt=json"))
        .asked()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(release_of(&v))
}
