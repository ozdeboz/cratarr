//! Kept on this device (decision 0038): what an item the app keeps holds now, so the app follows it. The app asks for
//! its kept items with the versions it last saw; each answers with its version (a hash of its tracks' ids, in order)
//! and, when that changed, its tracks with what the budget needs: their size as the file is, and as Cratarr's smaller
//! copy would be.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use serde::{Deserialize, Serialize};

use super::App;
use super::playlists::PlaylistRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Album,
    Playlist,
    Crate,
    Artist,
}

/// A kept item: an album (by its id), a playlist (by its name), a crate (by its id) or an artist (by name), of a source.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Item {
    pub kind: Kind,
    pub source: String,
    pub id: String,
}

impl Item {
    /// How the app and the answer name it: `kind:source/id`.
    pub fn key(&self) -> String {
        let kind = serde_json::to_value(self.kind).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
        format!("{kind}:{}/{}", self.source, self.id)
    }
}

#[derive(Debug, Deserialize)]
pub struct Ask {
    pub items: Vec<Item>,
    /// The version the app last saw, by key: an item still at it answers without its tracks.
    #[serde(default)]
    pub versions: HashMap<String, String>,
}

/// One track as the app keeps it (its download describes it as the page does), with its sizes.
#[derive(Debug, Clone, Serialize)]
pub struct Track {
    pub source: String,
    pub id: String,
    pub name: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_id: Option<String>,
    pub duration_secs: Option<f64>,
    /// Bytes as the file is; none when the library does not know it.
    pub size: Option<u64>,
    /// Bytes as Cratarr's smaller copy (Opus at 160 kb/s from a lossless file; a lossy file comes as it is).
    pub copy_size: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct Answer {
    pub key: String,
    /// Absent when the item could not be read (gone, or its server down): `error` says why.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Only when the version changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracks: Option<Vec<Track>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The Opus copies' rate (decision 0033): bytes per second.
const COPY_BYTES_PER_SEC: f64 = 160_000.0 / 8.0;

/// What each kept item holds now.
pub async fn ask(app: &App, ask: Ask) -> Vec<Answer> {
    let mut out = Vec::with_capacity(ask.items.len());
    for item in ask.items {
        let key = item.key();
        // A Bandcamp album is played, never kept on a device (decision 0071).
        if item.source == crate::bandcamp::SOURCE {
            out.push(Answer { key, version: None, tracks: None, error: Some(crate::bandcamp::NOT_KEPT.into()) });
            continue;
        }
        match rows_of(app, &item).await {
            Ok((source, rows)) => {
                let ids: Vec<(&str, Option<&str>)> = rows.iter().filter(|r| !r.missing).filter_map(|r| Some((r.id.as_deref()?, r.album_id.as_deref()))).collect();
                let version = version_of(&source, &ids);
                let tracks = (ask.versions.get(&key) != Some(&version)).then(|| tracks_of(&source, &rows));
                out.push(Answer { key, version: Some(version), tracks, error: None });
            }
            Err(e) => out.push(Answer { key, version: None, tracks: None, error: Some(e) }),
        }
    }
    out
}

/// An item's rows in its own order, and the source they are of.
async fn rows_of(app: &App, item: &Item) -> Result<(String, Vec<PlaylistRow>), String> {
    let source = Some(item.source.as_str());
    match item.kind {
        Kind::Album => app.playing.album_view(&item.source, &item.id).await.map(|v| (v.source, v.rows)).map_err(|e| e.to_string()),
        Kind::Playlist => app.playlists.rows(&item.id, source).await.map_err(|e| e.to_string()),
        Kind::Crate => app.playlists.crate_rows(&item.id, source, 0, usize::MAX, &super::CrateLook::default()).await.map(|p| (p.source, p.rows)).map_err(|e| e.to_string()),
        Kind::Artist => app.playing.artist_view(&item.source, &item.id).await.map(|v| (v.source, v.rows)).map_err(|e| e.to_string()),
    }
}

/// An item's version: its tracks and their albums (a track whose album becomes known is told to the app again).
fn version_of(source: &str, ids: &[(&str, Option<&str>)]) -> String {
    let mut h = DefaultHasher::new();
    source.hash(&mut h);
    ids.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// The rows the app can download (in the library, not missing), each once, with their sizes from Cratarr's own library.
fn tracks_of(source: &str, rows: &[PlaylistRow]) -> Vec<Track> {
    let sizes = sizes_of(rows.iter().filter_map(|r| r.id.as_deref()));
    let mut seen = std::collections::HashSet::new();
    rows.iter()
        .filter(|r| !r.missing)
        .filter_map(|r| {
            let id = r.id.clone()?;
            if !seen.insert(id.clone()) {
                return None;
            }
            let (size, lossless) = sizes.get(&id).copied().map_or((None, false), |(s, l)| (Some(s), l));
            let copy_size = if lossless { r.duration_secs.map(|d| (d * COPY_BYTES_PER_SEC) as u64) } else { size };
            Some(Track {
                source: source.to_string(),
                id,
                name: r.name.clone(),
                artist: r.artist.clone(),
                album: r.album.clone(),
                album_id: r.album_id.clone(),
                duration_secs: r.duration_secs,
                size,
                copy_size,
            })
        })
        .collect()
}

/// Each song's size and whether it is lossless (its copy is then Opus), from Cratarr's own library; songs of another
/// source are not there and get none.
fn sizes_of<'a>(ids: impl Iterator<Item = &'a str>) -> HashMap<String, (u64, bool)> {
    let mut out = HashMap::new();
    let Some(served) = crate::serve::instance() else { return out };
    let db = served.library.db();
    let Ok(mut st) = db.prepare_cached("SELECT size, suffix, kbps FROM songs WHERE id = ?1") else { return out };
    for id in ids {
        let row: rusqlite::Result<(i64, Option<String>, Option<i64>)> = st.query_row([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)));
        if let Ok((size, suffix, kbps)) = row {
            out.insert(id.to_string(), (size.max(0) as u64, lossless(suffix.as_deref(), kbps)));
        }
    }
    out
}

/// A lossless file, whose copy is Opus: FLAC, WAV, AIFF, or an M4A at a lossless rate (ALAC).
fn lossless(suffix: Option<&str>, kbps: Option<i64>) -> bool {
    match suffix.map(str::to_ascii_lowercase).as_deref() {
        Some("flac" | "wav" | "aiff" | "aif") => true,
        Some("m4a") => kbps.is_some_and(|k| k > 400),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_versions_and_what_counts_as_lossless() {
        let item: Item = serde_json::from_str(r#"{"kind":"crate","source":"cratarr","id":"c1"}"#).unwrap();
        assert_eq!(item.key(), "crate:cratarr/c1");
        let (a, b) = (("a", None), ("b", Some("al-1")));
        assert_eq!(version_of("cratarr", &[a, b]), version_of("cratarr", &[a, b]));
        assert_ne!(version_of("cratarr", &[a, b]), version_of("cratarr", &[b, a]), "the order counts (a cap keeps the first)");
        assert_ne!(version_of("cratarr", &[a, b]), version_of("cratarr", &[a, ("b", None)]), "an album found is told again");
        assert!(lossless(Some("FLAC"), None) && lossless(Some("m4a"), Some(900)) && !lossless(Some("m4a"), Some(256)) && !lossless(Some("mp3"), Some(320)));
    }

    #[test]
    fn tracks_once_each_without_the_missing_and_their_copy_size() {
        let row = |id: &str, missing: bool| PlaylistRow { id: Some(id.into()), missing, duration_secs: Some(100.0), ..Default::default() };
        let t = tracks_of("other", &[row("a", false), row("b", true), row("a", false), row("c", false)]);
        assert_eq!(t.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["a", "c"]);
        // Not in Cratarr's own library (no instance in tests): no sizes known.
        assert_eq!((t[0].size, t[0].copy_size), (None, None));
    }
}
