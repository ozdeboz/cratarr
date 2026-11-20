//! `renumber`'s file name and its reading of a Discogs release, in base: a track's file name as the sheets show it, which
//! the words' writer names too, and a Discogs release's media and lengths, as records' references and Discogs read them.
//! `renumber` re-exports them.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

pub(super) fn file_name(rel: &str) -> String {
    Path::new(rel).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| rel.to_string())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReleaseFacts {
    pub title: String,
    pub format: String,
    pub country: Option<String>,
    pub year: Option<u32>,
}

/// A release's medium: its tracks' titles and lengths (s), in order.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Medium {
    pub(super) format: String,
    pub(super) tracks: Vec<(String, Option<u32>)>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Release {
    /// MusicBrainz's id (none for a Discogs release).
    pub(super) id: String,
    pub(super) facts: ReleaseFacts,
    pub(super) media: Vec<Medium>,
}

/// "4:05" as seconds.
pub(super) fn duration(s: &str) -> Option<u32> {
    let mut secs = 0u32;
    for part in s.trim().split(':') {
        secs = secs * 60 + part.trim().parse::<u32>().ok()?;
    }
    (secs > 0).then_some(secs)
}

/// A Discogs tracklist position's medium: "2-5" disc 2, "B3" side B, "3" the one.
pub(super) fn medium_of(position: &str) -> String {
    let p = position.trim();
    if let Some((d, _)) = p.split_once(['-', '.']) {
        return d.to_string();
    }
    p.chars().take_while(|c| c.is_ascii_alphabetic()).collect()
}

pub(super) fn discogs_release(v: &Value) -> Release {
    let format = v["formats"].as_array().and_then(|f| f.first()).and_then(|f| f["name"].as_str()).unwrap_or_default().to_string();
    let mut media: Vec<(String, Medium)> = Vec::new();
    for t in v["tracklist"].as_array().into_iter().flatten().filter(|t| t["type_"].as_str().is_none_or(|k| k == "track")) {
        let key = medium_of(t["position"].as_str().unwrap_or_default());
        let track = (t["title"].as_str().unwrap_or_default().to_string(), t["duration"].as_str().and_then(duration));
        match media.last_mut() {
            Some((k, m)) if *k == key => m.tracks.push(track),
            _ => media.push((key, Medium { format: format.clone(), tracks: vec![track] })),
        }
    }
    Release {
        id: String::new(),
        facts: ReleaseFacts {
            title: v["title"].as_str().unwrap_or_default().to_string(),
            format,
            country: v["country"].as_str().map(str::to_string),
            year: v["year"].as_u64().filter(|y| *y > 0).map(|y| y as u32),
        },
        media: media.into_iter().map(|(_, m)| m).collect(),
    }
}
