//! The OpenSubsonic response shapes (decision 0030), JSON. The envelope, the error codes and the shapes of songs,
//! albums and artists follow nghe's `nghe-api` crate (https://github.com/vnghia/nghe, AGPL-3.0, Copyright vnghia) and
//! the OpenSubsonic specification (https://opensubsonic.netlify.app, Apache-2.0). Changed from nghe: plain serde types
//! on stable Rust (nghe derives them with nightly-only macros), string ids instead of UUIDs, the Subsonic 1.16.1 fields
//! nghe leaves out that clients read (`parent`, `isDir`, `path`, `genre`, `played`, `userRating`, `playCount`), and
//! errors answered with HTTP 200 as Navidrome does, since Subsonic clients read the envelope, not the status.

use serde::Serialize;
use serde_json::{Map, Value, json};

pub const API_VERSION: &str = "1.16.1";
pub const SERVER_TYPE: &str = "cratarr";

/// The `subsonic-response` envelope around a body (an object whose members are the response's).
pub fn ok(body: Value) -> Value {
    let mut root = envelope("ok");
    if let Value::Object(members) = body {
        root.extend(members);
    }
    json!({ "subsonic-response": root })
}

/// The envelope with an error. Subsonic's codes: 0 generic, 10 missing parameter, 20/30 version, 40 wrong login,
/// 41 token login not supported, 42/43/44 OpenSubsonic auth mechanism errors, 50 not allowed, 70 not found.
pub fn error(code: u32, message: &str) -> Value {
    let mut root = envelope("failed");
    root.insert("error".into(), json!({ "code": code, "message": message }));
    json!({ "subsonic-response": root })
}

fn envelope(status: &str) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("status".into(), status.into());
    m.insert("version".into(), API_VERSION.into());
    m.insert("type".into(), SERVER_TYPE.into());
    m.insert("serverVersion".into(), env!("CARGO_PKG_VERSION").into());
    m.insert("openSubsonic".into(), true.into());
    m
}

/// ISO 8601 in UTC from epoch seconds, as the API writes dates.
pub fn iso(epoch: i64) -> String {
    let days = epoch.div_euclid(86_400);
    let secs = epoch.rem_euclid(86_400);
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

/// A content type from a file's extension.
pub fn content_type(suffix: &str) -> &'static str {
    match suffix.to_ascii_lowercase().as_str() {
        "flac" => "audio/flac",
        "mp3" => "audio/mpeg",
        "m4a" | "mp4" | "aac" | "alac" => "audio/mp4",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/ogg; codecs=opus",
        "wav" => "audio/wav",
        "aif" | "aiff" => "audio/aiff",
        "wma" => "audio/x-ms-wma",
        "ape" => "audio/x-ape",
        "wv" => "audio/x-wavpack",
        "dsf" | "dff" => "audio/x-dsd",
        _ => "application/octet-stream",
    }
}

/// An artist reference (`artists`, `albumArtists` on songs and albums: OpenSubsonic).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ArtistRef {
    pub id: String,
    pub name: String,
}

/// A genre reference (`genres`: OpenSubsonic).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GenreRef {
    pub name: String,
}

/// A song's ReplayGain (OpenSubsonic `replayGain`): gains in dB, peaks linear (1.0 = full scale).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReplayGain {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_gain: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_gain: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_peak: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_peak: Option<f64>,
}

impl ReplayGain {
    /// None when it knows nothing; values to two decimals.
    pub fn of(track_gain: Option<f64>, track_peak: Option<f64>, album_gain: Option<f64>, album_peak: Option<f64>) -> Option<Self> {
        let r = |v: Option<f64>| v.map(|x| (x * 100.0).round() / 100.0);
        let g = ReplayGain { track_gain: r(track_gain), album_gain: r(album_gain), track_peak: track_peak.map(|x| (x * 10_000.0).round() / 10_000.0), album_peak: album_peak.map(|x| (x * 10_000.0).round() / 10_000.0) };
        (g != ReplayGain::default()).then_some(g)
    }
}

/// A song, as the API's `Child`.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Song {
    pub id: String,
    pub parent: String,
    pub is_dir: bool,
    pub title: String,
    pub album: String,
    pub artist: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    pub cover_art: String,
    pub size: u64,
    pub content_type: String,
    pub suffix: String,
    pub duration: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_rate: Option<u32>,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc_number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    pub album_id: String,
    pub artist_id: String,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub media_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starred: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub played: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_rating: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bpm: Option<u32>,
    /// OpenSubsonic's levelling (decision 0035): the analysis', else the file's own tags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_gain: Option<ReplayGain>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub music_brainz_id: Option<String>,
    pub artists: Vec<ArtistRef>,
    pub display_artist: String,
    pub album_artists: Vec<ArtistRef>,
    pub display_album_artist: String,
    pub genres: Vec<GenreRef>,
}

/// An album, as the API's `AlbumID3` (with `song` for `getAlbum`).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub artist_id: String,
    pub cover_art: String,
    pub song_count: u32,
    pub duration: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starred: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub played: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_rating: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub music_brainz_id: Option<String>,
    pub is_compilation: bool,
    pub artists: Vec<ArtistRef>,
    pub display_artist: String,
    pub genres: Vec<GenreRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub song: Option<Vec<Song>>,
    /// OpenSubsonic's release types ("Album", "Compilation"), from `release`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub release_types: Vec<String>,
    /// Its release type as Cratarr's own lists give it (0.9.6).
    #[serde(skip)]
    pub release: crate::source::ReleaseType,
}

/// An artist, as the API's `ArtistID3` (with `album` for `getArtist`).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Artist {
    pub id: String,
    pub name: String,
    pub cover_art: String,
    pub album_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starred: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_rating: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub music_brainz_id: Option<String>,
    pub sort_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<Vec<Album>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_envelope_is_subsonics() {
        let v = ok(json!({ "musicFolders": { "musicFolder": [] } }));
        let r = &v["subsonic-response"];
        assert_eq!((r["status"].as_str(), r["version"].as_str(), r["openSubsonic"].as_bool()), (Some("ok"), Some("1.16.1"), Some(true)));
        assert!(r["musicFolders"]["musicFolder"].as_array().is_some_and(Vec::is_empty), "empty lists are [], not null");
        let e = error(70, "Not found");
        assert_eq!(e["subsonic-response"]["status"], "failed");
        assert_eq!(e["subsonic-response"]["error"]["code"], 70);
    }

    #[test]
    fn dates_are_iso() {
        assert_eq!(iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso(1_792_065_086), "2026-10-15T11:51:26Z");
        assert_eq!(iso(951_782_400), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn songs_serialise_with_subsonics_names() {
        let s = Song { id: "tr-1".into(), kind: "music", media_type: "song", track: Some(3), ..Song::default() };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!((v["id"].as_str(), v["isDir"].as_bool(), v["type"].as_str(), v["track"].as_u64()), (Some("tr-1"), Some(false), Some("music"), Some(3)));
        assert!(v.get("year").is_none(), "unknown is left out");
        assert!(v["artists"].as_array().is_some(), "lists are always there");
    }
}
