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

pub mod cratarr;
pub mod jellyfin;
pub mod subsonic;

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
    /// Its tempo (decision 0035): the file's BPM tag, else the analysis'; none for a track without a beat.
    pub bpm: Option<u32>,
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
    /// Who asked for it, a queue request accepted (Play On's players only).
    pub asked_by: Option<String>,
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

/// When an album came and how it has been played, for the account the source plays for: the album panel's About.
/// Each `None` where the source cannot say.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AlbumListening {
    /// Epoch seconds.
    pub added: Option<i64>,
    /// Epoch seconds.
    pub played: Option<i64>,
    pub plays: Option<u32>,
}

/// How the account the source plays for used an album: its plays, when last played (epoch seconds), its rating (0:
/// none). The Library's orders and filters (decided 2026-10-16).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AlbumUse {
    pub plays: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub played: Option<i64>,
    pub rating: u8,
}

/// An album's track count, format ("FLAC"; `None` when mixed or unknown), lowest bitrate and length.
#[derive(Debug, Clone, Default, PartialEq, Serialize, serde::Deserialize)]
pub struct AlbumDetails {
    pub tracks: u32,
    pub format: Option<String>,
    pub bitrate_kbps: Option<u32>,
    /// Its tracks' lengths summed, in seconds (see `total_secs`); details kept from before it read as none.
    #[serde(default)]
    pub duration_secs: Option<f64>,
}

/// One of an album's tracks, in play order, with what it says of its album. `path` is the source's own.
#[derive(Debug, Clone, PartialEq)]
pub struct AlbumTrack {
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<u32>,
    pub path: Option<String>,
    pub duration_secs: Option<f64>,
}

/// An album's release type (0.9.6, the owner: the Library can hide EPs and singles): its primary type ("album", "ep",
/// "single", "other"; none: not known), where that came from ("musicbrainz", "discogs", "guess"), and its secondary
/// types ("compilation", "live"…). Cratarr's own library's; other servers' albums say none.
#[derive(Debug, Clone, Default, PartialEq, Serialize, serde::Deserialize)]
pub struct ReleaseType {
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub type_from: Option<String>,
    #[serde(default)]
    pub secondary: Vec<String>,
    /// Classical (0.10, the owner, 2026-10-30), a kind beside the release type: Cratarr's own library's records say
    /// whether they are (`app::classical`), other servers' say nothing; the mark where someone marked it (`classical`,
    /// `not`); a classical record's composers (keyed by MusicBrainz artist, else `name:<loose name>`) and performers,
    /// conductor, orchestra and soloists first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classical: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classical_mark: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub composers: Vec<ComposerRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub performers: Vec<String>,
}

/// A composer as a record, a track or a search result names them (0.10): the key the page addresses them by, and
/// their name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ComposerRef {
    pub key: String,
    pub name: String,
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
    /// Its tracks' lengths summed, in seconds; `None` where unknown (see `total_secs`). Albums kept from before it
    /// read as none.
    #[serde(default)]
    pub duration_secs: Option<f64>,
    /// The source's own tags or labels on the album, as they are.
    pub tags: Vec<String>,
    /// The library it was listed from (the album pages' library switches filter by it); none outside a listing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
    /// Its artists one by one, when the source credits several (the menu's Go to each); empty: `artist` alone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artists: Vec<String>,
    /// Its release type (0.9.6): `type`, `type_from`, `secondary`.
    #[serde(flatten, default)]
    pub release: ReleaseType,
}

/// What a source knows of an artist (the artist panel, decided 2026-10-16, as Symfonium's): the albums credited to them
/// and those they only appear on, newest first, and their tracks (those credited to them, and every track of their
/// albums) album by album.
#[derive(Debug, Clone, Default)]
pub struct ArtistWorks {
    pub albums: Vec<Album>,
    pub appears_on: Vec<Album>,
    pub tracks: Vec<ArtistTrack>,
    /// A favourite of the account the source plays for; `None` where the source keeps none.
    pub favorite: Option<bool>,
}

/// One of an artist's tracks, with its album and that account's plays of it.
#[derive(Debug, Clone)]
pub struct ArtistTrack {
    pub track: AlbumTrack,
    pub album_id: String,
    pub plays: u32,
    /// Epoch seconds.
    pub played: Option<i64>,
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

/// What one account did with one track, for an import into Cratarr's own user data (decision 0030, amended
/// 2026-10-15). Times are epoch seconds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackData {
    /// The source's id of the track.
    pub id: String,
    pub plays: u32,
    pub last_played: Option<i64>,
    pub favorite: bool,
    /// When it became a favourite, when the source keeps that.
    pub favorite_at: Option<i64>,
    /// 1 to 5; 0 when not rated.
    pub rating: u8,
    /// The track as the source names it, for finding it by name where the catalogue links none of the source's ids
    /// (a Subsonic server imported from without being a source, decision 0067).
    pub meta: Option<TrackMeta>,
}

/// A track's names as a source gives them (decision 0067).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackMeta {
    pub artist: String,
    pub title: String,
    pub album: Option<String>,
    pub length_secs: Option<f64>,
    /// MusicBrainz's recording.
    pub recording: Option<String>,
}

/// A page of [`TrackData`]. `next` asks for the page after it; `None` when the listing is done.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackDataPage {
    pub items: Vec<TrackData>,
    pub next: Option<String>,
    /// Tracks in the whole listing, when the source can tell (on the first page at least).
    pub total: Option<usize>,
}

/// An album as an import names it: the source's id, and one of its tracks (the source's id) or its folder (as the
/// source sees it) to find it by in Cratarr's library.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AlbumRef {
    pub id: String,
    pub track: Option<String>,
    pub path: Option<String>,
    /// Its name and album artist, to find it by where neither its track nor its folder does (decision 0067).
    pub name: Option<String>,
    pub artist: Option<String>,
}

/// An artist as an import names it: the source's id and the name, by which Cratarr finds it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ArtistRef {
    pub id: String,
    pub name: String,
}

/// One account's favourite albums and artists, every one, each with when it became a favourite when the source keeps
/// that.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FavoriteGroups {
    pub albums: Vec<(AlbumRef, Option<i64>)>,
    pub artists: Vec<(ArtistRef, Option<i64>)>,
}

/// A page of albums with when the source added them (epoch seconds); `next` and `total` as in [`TrackDataPage`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AddedPage {
    pub items: Vec<(AlbumRef, i64)>,
    pub next: Option<String>,
    pub total: Option<usize>,
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

    /// What plays on `player` (as `Playing::player` named it); without one, or on a source with no such notion, as
    /// `now_playing`. Carry-on (0.9.8) asks the player it stopped, not whichever is followed by then.
    fn now_playing_on<'a>(&'a self, player: Option<&'a str>) -> Fut<'a, Option<Playing>> {
        let _ = player;
        self.now_playing()
    }

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

    /// Optional: an artist's albums and tracks (the artist panel), by their name. `None`: the source cannot say.
    fn artist_works<'a>(&'a self, _name: &'a str) -> Fut<'a, Option<ArtistWorks>> {
        Box::pin(async { Ok(None) })
    }

    /// Optional: makes an artist a favourite or not (the artist panel's heart).
    fn set_artist_favorite<'a>(&'a self, _name: &'a str, _on: bool) -> Fut<'a, bool> {
        Box::pin(async { Err(anyhow::anyhow!("This server keeps no favourite artists.")) })
    }

    /// Optional: whether an artist is a favourite (the heart on their chip), by their name as the artist panel has it.
    /// `None`: the source keeps no favourite artists.
    fn artist_favorite<'a>(&'a self, _name: &'a str) -> Fut<'a, Option<bool>> {
        Box::pin(async { Ok(None) })
    }

    /// Optional: every album played or rated, by id (the Library's orders and filters by plays and rating), never kept:
    /// plays change. Empty where the source cannot say.
    fn album_uses(&self) -> Fut<'_, BTreeMap<String, AlbumUse>> {
        Box::pin(async { Ok(BTreeMap::new()) })
    }

    /// Optional: when an album came and how it has been played (the album panel's About), never kept: plays change.
    fn album_listening<'a>(&'a self, _album: &'a str) -> Fut<'a, AlbumListening> {
        Box::pin(async { Ok(AlbumListening::default()) })
    }

    /// Optional: play an album, from its first track, on the player this source follows. Returns a short label of
    /// the player, for the page to show.
    fn play_album<'a>(&'a self, _album_id: &'a str) -> Fut<'a, String> {
        Box::pin(async { anyhow::bail!("this source cannot play music") })
    }

    /// An album's tracks in play order: its view in the panel, and the local player (decision 0022).
    fn tracks_of_album<'a>(&'a self, _album_id: &'a str) -> Fut<'a, Vec<AlbumTrack>> {
        Box::pin(async { anyhow::bail!("this server cannot list an album's tracks") })
    }

    /// Several albums' tracks, each with its album's id: the albums in the order given, each one's in play order (0.9.4:
    /// Select › All's menu on thousands of albums). One album after another where the source has no better way; an
    /// album it cannot read left out.
    fn tracks_of_albums<'a>(&'a self, albums: &'a [String]) -> Fut<'a, Vec<(String, AlbumTrack)>> {
        Box::pin(async move {
            let mut out = Vec::new();
            for a in albums {
                match self.tracks_of_album(a).await {
                    Ok(tracks) => out.extend(tracks.into_iter().map(|t| (a.clone(), t))),
                    Err(e) => tracing::debug!("album {a}: its tracks not read: {e:#}"),
                }
            }
            Ok(out)
        })
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

    /// Optional (decision 0014, amended 2026-10-15): "your account": the one whose playlists are Cratarr's, kept in
    /// step always. Its setting, else the server's only account or its administrator: stable, whoever plays.
    fn sync_account(&self) -> Fut<'_, String> {
        Box::pin(async { anyhow::bail!("this server keeps no playlists of its own") })
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

    /// Optional: count one play, played at `at` (epoch seconds): a player that was offline tells its plays late
    /// (decision 0033). A server that cannot date a play counts it now.
    fn count_play_at<'a>(&'a self, track: &'a str, _at: i64) -> Fut<'a, ()> {
        self.count_play(track)
    }

    /// Optional: which of these tracks are favourites of the account the source plays for, in one go (the hearts on
    /// a list's rows). Tracks it cannot tell about are left out.
    fn favorites<'a>(&'a self, _tracks: &'a [String]) -> Fut<'a, BTreeMap<String, bool>> {
        Box::pin(async { Ok(BTreeMap::new()) })
    }

    /// Optional: every track the account the source plays for has as a favourite (its ids), for the Favourites crate
    /// (decision 0024).
    fn favorite_tracks(&self) -> Fut<'_, Vec<String>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    /// Optional: every album the account the source plays for has as a favourite (its ids): the Library's Favourites
    /// only, and the album panel's heart. A server that cannot list them says so.
    fn favorite_albums(&self) -> Fut<'_, Vec<String>> {
        Box::pin(async { anyhow::bail!("this server cannot list favourite albums") })
    }

    /// Optional: every artist the account the source plays for has as a favourite, by name as its library names them
    /// (0.9.15-1, the owner: the Library's filter "whole and complete": Artists' Favourites only). A server that cannot
    /// list them says so.
    fn favorite_artists(&self) -> Fut<'_, Vec<String>> {
        Box::pin(async { anyhow::bail!("this server cannot list favourite artists") })
    }

    /// Optional: the account's rating of an album, 0 (none) to 5, as Subsonic apps give it; `None`: the server keeps no
    /// ratings (the menu then offers none).
    fn album_rating<'a>(&'a self, _album: &'a str) -> Fut<'a, Option<u8>> {
        Box::pin(async { Ok(None) })
    }

    /// Optional: rates an album, 0 (none) to 5.
    fn set_album_rating<'a>(&'a self, _album: &'a str, _stars: u8) -> Fut<'a, ()> {
        Box::pin(async { anyhow::bail!("this server keeps no ratings") })
    }

    /// Makes an album a favourite or not (the album panel's heart): by default as any of the server's items is.
    fn set_album_favorite<'a>(&'a self, album: &'a str, on: bool) -> Fut<'a, bool> {
        self.set_favorite(album, on)
    }

    /// Optional: which playlists (by name) are favourites of the account the source plays for (decision 0023, as
    /// Finamp's playlist hearts). A Cratarr playlist can be there more than once (imported, and an account's own copy):
    /// it is a favourite when any of them is.
    fn playlist_favorites(&self) -> Fut<'_, BTreeMap<String, bool>> {
        Box::pin(async { Ok(BTreeMap::new()) })
    }

    /// Optional: makes the playlists with this name (every copy) a favourite or not.
    fn set_playlist_favorite<'a>(&'a self, _name: &'a str, _on: bool) -> Fut<'a, ()> {
        Box::pin(async { anyhow::bail!("this server keeps no favourite playlists") })
    }

    /// Whether the track is a favourite of the account the source plays for; `None` when it cannot tell.
    fn favorite<'a>(&'a self, _track: &'a str) -> Fut<'a, Option<bool>> {
        Box::pin(async { Ok(None) })
    }

    fn set_favorite<'a>(&'a self, _track: &'a str, _on: bool) -> Fut<'a, bool> {
        Box::pin(async { anyhow::bail!("this server has no favourites") })
    }

    /// Whether this server's playlists can hold a track twice. Jellyfin's cannot: the playlist sync then compares and
    /// writes the first copies only, and keeps the file's repeats (decision 0025, amended 2026-10-16).
    fn playlists_keep_repeats(&self) -> bool {
        true
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

    /// `transport` on `player`; without one, or on a source with no such notion, as `transport`.
    fn transport_on<'a>(&'a self, command: Transport, player: Option<&'a str>) -> Fut<'a, String> {
        let _ = player;
        self.transport(command)
    }

    fn play_tracks<'a>(&'a self, _ids: &'a [String], _how: PlayHow) -> Fut<'a, String> {
        Box::pin(async { anyhow::bail!("this server cannot play music") })
    }

    /// Optional: play these tracks now on the player the page follows, the first from `start_secs` in (to carry on
    /// where a moved file was). Returns the player's label.
    fn play_from<'a>(&'a self, _ids: &'a [String], _start_secs: f64) -> Fut<'a, String> {
        Box::pin(async { anyhow::bail!("this server cannot play music from a position") })
    }

    /// `play_from` on `player`; without one, or on a source with no such notion, as `play_from`.
    fn play_from_on<'a>(&'a self, ids: &'a [String], start_secs: f64, player: Option<&'a str>) -> Fut<'a, String> {
        let _ = player;
        self.play_from(ids, start_secs)
    }

    /// Optional: an album's cover image (content type, bytes). Lets Cratarr show covers of music outside the playlist
    /// folder without giving the page the source's address or key.
    fn album_cover<'a>(&'a self, _album_id: &'a str) -> Fut<'a, Option<(String, Vec<u8>)>> {
        Box::pin(async { Ok(None) })
    }

    /// Whether this source implements the three reads below, which import an account's plays, favourites, ratings and
    /// its albums' added dates into Cratarr's own user data (decision 0030, amended 2026-10-15).
    fn imports(&self) -> bool {
        false
    }

    /// Optional: a page of what `user` did with tracks. Every track the user played or made a favourite is in the
    /// listing at least once (twice is harmless). `cursor`: `None` for the first page, then the page's `next`. One
    /// page is one request or a few, never several at once: imports run beside everything else.
    fn track_data<'a>(&'a self, _user: &'a str, _cursor: Option<&'a str>) -> Fut<'a, TrackDataPage> {
        Box::pin(async { anyhow::bail!("this server cannot list an account's plays") })
    }

    /// Optional: every album and artist `user` made a favourite.
    fn favorite_groups<'a>(&'a self, _user: &'a str) -> Fut<'a, FavoriteGroups> {
        Box::pin(async { anyhow::bail!("this server cannot list an account's favourites") })
    }

    /// Optional: a page of the source's albums with when it added them; `cursor` as in [`Source::track_data`].
    fn albums_added<'a>(&'a self, _cursor: Option<&'a str>) -> Fut<'a, AddedPage> {
        Box::pin(async { anyhow::bail!("this server cannot say when it added its albums") })
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

pub static KINDS: &[Kind] = &[cratarr::KIND, jellyfin::KIND, subsonic::KIND];

/// An album's length from its tracks' lengths, in seconds: their sum, or `None` when it has no tracks or any track's
/// length is unknown (a sum of some would put the album too short in a sort by length).
pub fn total_secs(lengths: impl IntoIterator<Item = Option<f64>>) -> Option<f64> {
    let mut n = 0;
    let sum = lengths.into_iter().try_fold(0.0, |sum, l| {
        n += 1;
        l.filter(|s| *s > 0.0).map(|s| sum + s)
    })?;
    (n > 0).then_some(sum)
}

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

    #[test]
    fn an_albums_length_is_its_tracks_summed_and_unknown_when_one_is() {
        assert_eq!(total_secs([Some(200.5), Some(99.5)]), Some(300.0));
        assert_eq!(total_secs([Some(200.0), None]), None, "one unknown: the whole is unknown");
        assert_eq!(total_secs([Some(200.0), Some(0.0)]), None, "a length of nothing is not known");
        assert_eq!(total_secs(Vec::new()), None, "no tracks");
    }

    #[test]
    fn an_album_kept_from_before_its_length_still_reads() {
        let old = r#"{"id":"a1","title":"Homogenic","artist":"Björk","year":1997,"added":null,"path":null,"tracks":10,
            "format":"FLAC","bitrate_kbps":900,"tags":[],"type":"album"}"#;
        let a: Album = serde_json::from_str(old).unwrap();
        assert_eq!((a.title.as_str(), a.tracks, a.duration_secs), ("Homogenic", 10, None));
        assert_eq!(a.release.kind.as_deref(), Some("album"));
        let json = serde_json::to_value(&a).unwrap();
        assert!(json["duration_secs"].is_null(), "always sent, null where unknown");
        let timed = Album { duration_secs: Some(2640.5), ..a };
        assert_eq!(serde_json::to_value(&timed).unwrap()["duration_secs"], 2640.5);
        let d: AlbumDetails = serde_json::from_str(r#"{"tracks":10,"format":"FLAC","bitrate_kbps":900}"#).unwrap();
        assert_eq!((d.tracks, d.duration_secs), (10, None));
    }

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
            // With nothing filled in, a plugin that needs settings must refuse cleanly rather than panic.
            if fields.iter().any(|f| f.required) {
                assert!((k.connect)(&Values::new(), false).is_err());
                assert!((k.connect)(&Values::new(), true).is_err());
            }
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
