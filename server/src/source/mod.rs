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
    /// Its folders as the server describes them; only shown (to say what to mount), never used to find files.
    pub paths: Vec<String>,
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

/// What a media server knows about a track's file and plays: the player's info chips (decision 0019). Each `None` when
/// it does not know.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct TrackInfo {
    pub play_count: Option<u32>,
    pub codec: Option<String>,
    pub bitrate_kbps: Option<u32>,
    pub bit_depth: Option<u32>,
    pub sample_rate_hz: Option<u32>,
    pub explicit: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Playing {
    pub track: Track,
    pub progress: Option<Progress>,
    /// The player it plays on, as the page names it ("Finamp on Lenovo Tab"); the device the controls act on.
    pub player: Option<String>,
    /// When this track started, epoch seconds, if known. With several sources playing, "automatic" follows the
    /// most recent start.
    pub started_at: Option<i64>,
    /// Whether the player's account has marked the track a favourite; `None` when that is not known.
    pub favorite: Option<bool>,
    /// What plays next, in order, as far as the player reports: the page shows the next track at once when Next is
    /// pressed, before the player confirms.
    pub upcoming: Vec<Upcoming>,
}

/// A track the player plays after the current one.
#[derive(Debug, Clone, PartialEq)]
pub struct Upcoming {
    pub track: Track,
    pub duration_secs: Option<f64>,
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

/// A server's library scan: its progress while it runs, and when it last finished (epoch seconds).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LibraryScan {
    pub running: Option<f64>,
    pub last_end: Option<i64>,
}

/// An album as listed, before its details. `stamp` changes whenever the source saves the album again: its details
/// are read again then.
#[derive(Debug, Clone, PartialEq)]
pub struct Listed {
    pub album: Album,
    pub stamp: Option<String>,
}

/// An album's track count, format ("FLAC"; `None` when mixed or unknown) and lowest bitrate.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct AlbumDetails {
    pub tracks: u32,
    pub format: Option<String>,
    pub bitrate_kbps: Option<u32>,
}

/// An album as a source sees it. Paths are the source's own (see `placement`).
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
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

/// One of a user's own playlists on the server (decision 0012).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnPlaylist {
    pub id: String,
    pub name: String,
    /// Changes whenever the server saves the playlist; compared to notice edits made elsewhere.
    pub stamp: Option<String>,
}

/// How tracks are sent to a player (decision 0006).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayHow {
    /// Replace what plays, in the given order.
    Now,
    /// Right after the current track, in the given order.
    Next,
    /// At the end of the queue, in the given order.
    Last,
}

/// One page of a library's tracks: (track id, path as the server sees it).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackPage {
    pub items: Vec<(String, String)>,
    /// Tracks in the whole listing (all pages).
    pub total: usize,
}

/// A player control, as sent from the now-playing header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Transport {
    Previous,
    PlayPause,
    Next,
    /// Stop playing (Keep and Replace, while they move the album that plays: decision 0013).
    Stop,
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

    /// Optional: a library's own folders, as the source writes track paths. Lets one broad mount hold several
    /// libraries: without it, the mounted folder a library's tracks are found in counts as its folder.
    fn library_roots<'a>(&'a self, _library_id: &'a str) -> Fut<'a, Vec<String>> {
        Box::pin(async { Ok(Vec::new()) })
    }

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

    /// Optional: albums in a library, newest first, at most `limit`, without their details: listing is cheap,
    /// details are not (see `album_summary`).
    fn albums<'a>(&'a self, _library_id: &'a str, _limit: usize) -> Fut<'a, Vec<Listed>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    /// Optional: an album's track count, format and lowest bitrate, as the source knows them. Costly for some
    /// sources: Cratarr takes them from its own catalogue where it can, and keeps them (decision 0011).
    fn album_summary<'a>(&'a self, _album: &'a str) -> Fut<'a, Option<AlbumDetails>> {
        Box::pin(async { Ok(None) })
    }

    /// Optional: play an album, from its first track, on the player this source follows. Returns a short label of
    /// the player, for the page to show.
    fn play_album<'a>(&'a self, _album_id: &'a str) -> Fut<'a, String> {
        Box::pin(async { anyhow::bail!("this source cannot play music") })
    }

    /// An album's track ids in play order, for the local player (decision 0022).
    fn album_track_ids<'a>(&'a self, _album_id: &'a str) -> Fut<'a, Vec<String>> {
        Box::pin(async { anyhow::bail!("this server cannot list an album's tracks") })
    }

    /// Whether this source implements [`Source::track_paths`], which the catalogue uses to know its track ids.
    fn lists_tracks(&self) -> bool {
        false
    }

    /// Optional: a page of a library's tracks with their paths, in a stable order. `since` (epoch seconds) limits
    /// the listing to tracks the server changed after then; `None` lists them all.
    fn track_paths<'a>(&'a self, _library_id: &'a str, _since: Option<i64>, _start: usize, _limit: usize) -> Fut<'a, TrackPage> {
        Box::pin(async { Ok(TrackPage::default()) })
    }

    /// Optional (decision 0014): the accounts that can have playlists of their own, as (id, name).
    fn users(&self) -> Fut<'_, Vec<(String, String)>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    /// Optional (decision 0012): a user's own playlists (not ones imported from files), with their change stamps.
    fn own_playlists<'a>(&'a self, _user: &'a str) -> Fut<'a, Vec<OwnPlaylist>> {
        Box::pin(async { anyhow::bail!("this server keeps no playlists of its own") })
    }

    /// Optional (decision 0012): a playlist's tracks (this source's ids), in order.
    fn playlist_tracks<'a>(&'a self, _user: &'a str, _playlist: &'a str) -> Fut<'a, Vec<String>> {
        Box::pin(async { anyhow::bail!("this server keeps no playlists of its own") })
    }

    /// Optional (decision 0012): a new private playlist for `user` with these tracks. Returns its id.
    fn create_playlist<'a>(&'a self, _user: &'a str, _name: &'a str, _ids: &'a [String]) -> Fut<'a, String> {
        Box::pin(async { anyhow::bail!("this server keeps no playlists of its own") })
    }

    /// Optional: a short summary of what the server's players do now (track, paused, queue), compared to notice a
    /// change cheaply (the page's live channel). `None`: this server cannot say; the whole state is asked instead.
    fn players(&self) -> Fut<'_, Option<String>> {
        Box::pin(async { Ok(None) })
    }

    /// Optional: mark a track a favourite (or not) for the account of the player playing it. Returns the state now.
    /// Safe to repeat.
    /// Optional: what the server knows about the track's file and plays (the info chips); `None` when nothing.
    fn track_info<'a>(&'a self, _track: &'a str) -> Fut<'a, Option<TrackInfo>> {
        Box::pin(async { Ok(None) })
    }

    /// Optional: the player the page follows plays from this track of its queue on (what came between is skipped).
    /// Returns the player's label.
    fn jump_to<'a>(&'a self, _track: &'a str) -> Fut<'a, String> {
        Box::pin(async { anyhow::bail!("this server's player cannot jump in its queue") })
    }

    /// Optional: count one play of the track (Cratarr played it itself, decision 0022).
    fn count_play<'a>(&'a self, _track: &'a str) -> Fut<'a, ()> {
        Box::pin(async { anyhow::bail!("this server cannot count plays") })
    }

    /// Optional: which of these tracks are favourites of the account the source plays for, in one go (the hearts on
    /// a list's rows). Tracks it cannot tell about are left out.
    fn favorites<'a>(&'a self, _tracks: &'a [String]) -> Fut<'a, BTreeMap<String, bool>> {
        Box::pin(async { Ok(BTreeMap::new()) })
    }

    /// Whether the track is a favourite of the account the source plays for; `None` when it cannot tell.
    fn favorite<'a>(&'a self, _track: &'a str) -> Fut<'a, Option<bool>> {
        Box::pin(async { Ok(None) })
    }

    fn set_favorite<'a>(&'a self, _track: &'a str, _on: bool) -> Fut<'a, bool> {
        Box::pin(async { anyhow::bail!("this server has no favourites") })
    }

    /// Optional (decision 0012): make a playlist hold exactly these tracks, in this order. Safe to repeat.
    fn set_playlist_tracks<'a>(&'a self, _user: &'a str, _playlist: &'a str, _ids: &'a [String]) -> Fut<'a, ()> {
        Box::pin(async { anyhow::bail!("this server keeps no playlists of its own") })
    }

    /// Optional (decision 0012): rename a playlist. Returns its id afterwards, which a server may change. Safe to
    /// repeat, and after being cut short.
    fn rename_playlist<'a>(&'a self, _user: &'a str, _playlist: &'a str, _name: &'a str) -> Fut<'a, String> {
        Box::pin(async { anyhow::bail!("this server keeps no playlists of its own") })
    }

    /// Optional (decision 0012): delete a playlist of the user's own.
    fn delete_playlist<'a>(&'a self, _playlist: &'a str) -> Fut<'a, ()> {
        Box::pin(async { anyhow::bail!("this server keeps no playlists of its own") })
    }

    /// Optional (decision 0013): what each user did with an item (plays, last played, favourite, rating), as
    /// (user, data) for the users with something to keep. Read before Cratarr moves the item's file: a server forgets
    /// a moved file's item, and with it this.
    fn user_data<'a>(&'a self, _item: &'a str) -> Fut<'a, Vec<(String, serde_json::Value)>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    /// One user's data on two items that become one (Replace: the old album's track and its upgrade in Incoming), as
    /// `user_data` returned them. Without a way to combine them, the first is kept.
    fn combine_user_data(&self, a: &serde_json::Value, _b: &serde_json::Value) -> serde_json::Value {
        a.clone()
    }

    /// Optional (decision 0013): give an item a user's data as `user_data` returned it. Safe to repeat.
    fn set_user_data<'a>(&'a self, _item: &'a str, _user: &'a str, _data: &'a serde_json::Value) -> Fut<'a, ()> {
        Box::pin(async { anyhow::bail!("this server keeps no play data") })
    }

    /// Optional: the server's library scan, to notice one that no longer moves (see `app::library::Watch`).
    fn library_scan(&self) -> Fut<'_, LibraryScan> {
        Box::pin(async { Ok(LibraryScan::default()) })
    }

    /// Optional: files appeared (`true`) or disappeared (`false`), paths as the server sees them, so it notices a
    /// move at once instead of at its next scan.
    fn files_changed<'a>(&'a self, _changes: &'a [(String, bool)]) -> Fut<'a, ()> {
        Box::pin(async { Ok(()) })
    }

    /// Optional: play tracks (this source's ids, in order) on the player it follows. Returns the player's label.
    /// Optional: previous / play-pause / next on the player that plays now (or the chosen one). Returns its label.
    fn transport<'a>(&'a self, _command: Transport) -> Fut<'a, String> {
        Box::pin(async { anyhow::bail!("this server cannot control its players") })
    }

    fn play_tracks<'a>(&'a self, _ids: &'a [String], _how: PlayHow) -> Fut<'a, String> {
        Box::pin(async { anyhow::bail!("this server cannot play music") })
    }

    /// Optional: play these tracks now on the player the page follows, the first from `start_secs` in (to carry on
    /// where a moved file was). Returns the player's label.
    fn play_from<'a>(&'a self, _ids: &'a [String], _start_secs: f64) -> Fut<'a, String> {
        Box::pin(async { anyhow::bail!("this server cannot play music from a position") })
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
