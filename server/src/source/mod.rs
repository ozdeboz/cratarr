//! Music sources: the servers and players Cratarr works alongside. Each kind of source is a plugin compiled into
//! the binary: a module that implements [`Source`] and is listed in [`KINDS`]. See decision 0005.
//!
//! Writing a plugin:
//! - Describe the settings it needs with [`Field`]s. The Settings page draws the form from them, so a plugin needs
//!   no UI code. A secret names the fields it is tied to (`bound_to`); the core then refuses to reuse a saved secret
//!   once one of those changes, so a stored key is never sent somewhere new.
//! - `connect` turns saved values into a [`Source`]. With `partial` set it must work from just enough to reach the
//!   server (the page is still filling in the rest) and fill the dynamic choices.
//! - Report what is playing and which library a track is in, with the track's path as the source sees it. The core
//!   owns everything about playlist files: it works out where that path lies in the playlist folder (`placement`),
//!   decides whether a track may be added, and tells the source when playlist files change.
//! - Optional extras have default implementations (see [`Source::album_details`]).

pub mod jellyfin;

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use anyhow::Result;
use serde::Serialize;

use crate::nowplaying::Progress;

/// What a source's async methods return. Boxed so `Source` can be used as `dyn Source`.
pub type Fut<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// Saved settings of one source: field key to a string, or a list of strings for [`FieldKind::MultiChoice`].
pub type Values = BTreeMap<String, serde_json::Value>;

/// One setting a plugin asks for.
#[derive(Debug, Clone, Serialize)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: FieldKind,
    pub required: bool,
    pub help: Option<&'static str>,
    /// Secrets only: changing any of these fields needs the secret entered again.
    pub bound_to: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)] // part of the plugin contract; the Jellyfin plugin does not need every kind
pub enum FieldKind {
    Url,
    Text,
    /// Write-only: never sent back to the page, only whether it is set.
    Secret,
    /// One of the choices [`Source::options`] gives for this key.
    Choice,
    /// Several of the choices [`Source::options`] gives for this key.
    MultiChoice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Choice {
    pub value: String,
    pub label: String,
}

/// A music library on the source. Only libraries inside the playlist folder can ever be used (see `placement`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Library {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// The source's id of the track's album, when it has one.
    pub album_id: Option<String>,
    /// As the source sees it.
    pub path: Option<String>,
    /// `None` when the source cannot tell; such a track is never addable.
    pub library_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Playing {
    pub track: Track,
    pub progress: Option<Progress>,
    /// When this track started, epoch seconds, if known. With several sources playing, "automatic" follows the
    /// most recent start.
    pub started_at: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Status {
    pub connected: bool,
    pub server_name: Option<String>,
    pub version: Option<String>,
    /// A short extra line, e.g. which user is followed.
    pub detail: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistChange {
    Created,
    Deleted,
    /// Entries were added, removed or replaced.
    Edited,
}

/// A playlist file that changed, for the source to pick up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistEvent {
    /// File name without `.m3u`, which is also the playlist's name.
    pub name: String,
    /// The file as the source sees it; `None` while the playlist folder's place on the source is unknown.
    pub path: Option<String>,
    pub change: PlaylistChange,
}

/// An album as a source sees it. Paths are the source's own (see `placement`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Album {
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub year: Option<u32>,
    /// When the source added it, epoch seconds (for newest-first).
    pub added: Option<i64>,
    pub path: Option<String>,
    pub tracks: u32,
    /// e.g. "FLAC", "MP3"; `None` when mixed or unknown.
    pub format: Option<String>,
    /// The lowest track bitrate, so a weak track is not hidden by an average.
    pub bitrate_kbps: Option<u32>,
    /// The source's own tags or labels on the album, as they are.
    pub tags: Vec<String>,
}

pub trait Source: Send + Sync {
    fn status(&self) -> Fut<'_, Status>;

    /// Choices for this plugin's [`FieldKind::Choice`] and [`FieldKind::MultiChoice`] fields, by field key.
    fn options(&self) -> Fut<'_, BTreeMap<String, Vec<Choice>>>;

    /// Music libraries only.
    fn libraries(&self) -> Fut<'_, Vec<Library>>;

    /// Paths (as the source sees them) of up to `n` tracks in a library, used to find where the library lies in
    /// the playlist folder.
    fn sample_paths<'a>(&'a self, library_id: &'a str, n: usize) -> Fut<'a, Vec<String>>;

    /// What is playing now, following the player chosen in the settings when the plugin has that notion.
    fn now_playing(&self) -> Fut<'_, Option<Playing>>;

    /// One track by the id this source gave it.
    fn track<'a>(&'a self, id: &'a str) -> Fut<'a, Track>;

    /// Best effort: tell the source about changed playlist files.
    fn playlists_changed(&self, events: Vec<PlaylistEvent>) -> Fut<'_, ()>;

    /// Optional: what the source knows about the tracks of an album folder (by file name), for the Albums page.
    fn album_details<'a>(&'a self, _folder: &'a str) -> Fut<'a, BTreeMap<String, crate::albums::Known>> {
        Box::pin(async { Ok(BTreeMap::new()) })
    }

    /// Whether this source implements [`Source::albums`]; without it the Incoming page says so instead of showing an
    /// empty list as if that were the truth.
    fn lists_albums(&self) -> bool {
        false
    }

    /// Optional: albums in a library, newest first, at most `limit`.
    fn albums<'a>(&'a self, _library_id: &'a str, _limit: usize) -> Fut<'a, Vec<Album>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    /// Optional: play an album, from its first track, on the player this source follows. Returns a short label of
    /// the player, for the page to show.
    fn play_album<'a>(&'a self, _album_id: &'a str) -> Fut<'a, String> {
        Box::pin(async { anyhow::bail!("this source cannot play music") })
    }

    /// Optional: an album's cover image (content type, bytes). Lets Cratarr show covers of music outside the playlist
    /// folder without giving the page the source's address or key.
    fn album_cover<'a>(&'a self, _album_id: &'a str) -> Fut<'a, Option<(String, Vec<u8>)>> {
        Box::pin(async { Ok(None) })
    }
}

/// A plugin: what it is called, what it needs, and how to connect.
pub struct Kind {
    /// Stable; stored in the settings file.
    pub id: &'static str,
    pub label: &'static str,
    pub fields: fn() -> Vec<Field>,
    /// Build a source from saved values. `partial`: only enough to reach the server is required (for filling choices
    /// while the page is still being filled in); otherwise every required field must be set.
    pub connect: fn(&Values, partial: bool) -> Result<Box<dyn Source>>,
}

pub static KINDS: &[Kind] = &[jellyfin::KIND];

pub fn kind(id: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|k| k.id == id)
}

/// A string value, trimmed; `None` when missing or blank.
pub fn text<'a>(values: &'a Values, key: &str) -> Option<&'a str> {
    values.get(key).and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty())
}

/// A list value; a lone string counts as a one-item list.
pub fn list(values: &Values, key: &str) -> Vec<String> {
    match values.get(key) {
        Some(serde_json::Value::Array(a)) => a.iter().filter_map(|v| v.as_str()).map(str::to_string).collect(),
        Some(serde_json::Value::String(s)) if !s.trim().is_empty() => vec![s.trim().to_string()],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The contract every plugin must keep; a new plugin gets these checks for free.
    #[test]
    fn every_plugin_describes_its_fields_consistently() {
        let mut ids = std::collections::HashSet::new();
        for k in KINDS {
            assert!(ids.insert(k.id), "duplicate plugin id {}", k.id);
            assert!(!k.id.is_empty() && k.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
            let fields = (k.fields)();
            let keys: Vec<&str> = fields.iter().map(|f| f.key).collect();
            for f in &fields {
                assert_eq!(keys.iter().filter(|x| **x == f.key).count(), 1, "{}: duplicate field {}", k.id, f.key);
                assert!(
                    f.bound_to.is_empty() || f.kind == FieldKind::Secret,
                    "{}: only secrets can be bound ({})",
                    k.id,
                    f.key
                );
                for b in f.bound_to {
                    assert!(keys.contains(b), "{}: {} is bound to unknown field {b}", k.id, f.key);
                }
            }
            // With nothing filled in, connecting must fail cleanly rather than panic.
            assert!((k.connect)(&Values::new(), false).is_err());
            assert!((k.connect)(&Values::new(), true).is_err());
        }
    }

    #[test]
    fn values_read_as_text_and_lists() {
        let v: Values = serde_json::from_str(r#"{"a":" x ","b":["1","2"],"c":"one","d":"  "}"#).unwrap();
        assert_eq!(text(&v, "a"), Some("x"));
        assert_eq!(text(&v, "d"), None);
        assert_eq!(text(&v, "zz"), None);
        assert_eq!(list(&v, "b"), vec!["1", "2"]);
        assert_eq!(list(&v, "c"), vec!["one"]);
        assert!(list(&v, "d").is_empty());
    }
}
