//! Play On (decision 0031): the players Cratarr commands, and what each plays.
//!
//! A player is the Android app (android/). It keeps `GET /api/players/{name}/connect` open (server-sent events: the
//! commands for it) and reports what it plays with `PUT /api/players/{name}/state`. Home Assistant, or anything else,
//! reads `GET /api/players`, commands a player with `POST /api/players/{name}/command`, and is told of every change:
//! the player list is POSTed to each webhook (`GET`/`PUT /api/players/settings`, kept in `players.json`).
//!
//! Players live in memory: one whose connection is closed is offline; one not heard of since the start is unknown.

use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};

use super::queue::{Queue, Repeat};
use super::radio::{AutoMode, Autoplay, AutoplayMode, Mode as RadioMode, RadioBy, from_old};
use super::{PlaylistService, ToggleError};

/// A player not heard from (a report, the 30 s heartbeat) for this long is offline, even if its connection still looks
/// open: a device that lost power or the network leaves the connection open until a write fails, minutes later.
const LEASE: Duration = Duration::from_secs(75);
/// Changes reaching the webhooks within this long go out as one.
const SETTLE: Duration = Duration::from_millis(300);
/// How long a webhook may take to answer.
const WEBHOOK_TIMEOUT: Duration = Duration::from_secs(5);

/// A track as players hold it: the page's own description (`LocalTrack` in web/src/lib/localPlayer.svelte.ts).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QueueTrack {
    /// Its entry in the queue (decision 0033): an id of its own, never reused, apart from the track's.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub entry: String,
    /// `next`: added by hand (Play next, Add to queue), not part of the album or playlist that plays (0033's lanes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane: Option<String>,
    /// Its place in the album or playlist as it was played, before any shuffle: turning shuffle off orders by it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    pub source: String,
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub album_id: Option<String>,
    /// Its length, for the queue's rows and the time left; players keep it as they keep the rest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,
}

/// What a player reports.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Report {
    /// The playing track's place in the queue; -1 with nothing loaded.
    pub index: i64,
    /// The entry that plays (decision 0033); apps from before it say only `index`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    /// The queue's version in the last command the player took: a queue it reports from before Cratarr's latest edit
    /// reached it is not taken (that edit is on its way). Apps from before it leave it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u64>,
    /// Its repeat mode, as it applies it (it may change it itself: a new play turns repeat one off).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<Repeat>,
    /// Its sleep timer, as it keeps it; none set (or one that fired) is left out or null.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sleep: Option<Sleep>,
    /// Its Radio (decision 0036, amended): on or off. Apps from before the amendment report their Autoplay here, as an
    /// object; apps from before 0036 leave it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radio: Option<ReportedRadio>,
    /// What its Radio's inserts are like, and how many follow each own track; left out: not said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radio_by: Option<RadioBy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radio_count: Option<u8>,
    /// Whether its Radio and Autoplay leave favourites out; left out: not said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_favourites: Option<bool>,
    /// Its Autoplay: the setting, or null while off; left out: not said.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    pub autoplay: Option<Option<Autoplay>>,
    pub position_ms: u64,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    pub playing: bool,
    /// 0 to 100.
    #[serde(default = "full_volume")]
    pub volume: u8,
    #[serde(default)]
    pub ducked: bool,
    /// The whole queue when it changed since the last report; `None` keeps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue: Option<Vec<QueueTrack>>,
}

fn full_volume() -> u8 {
    100
}

/// A sleep timer: the player pauses at `at_ms` (epoch milliseconds), or when the playing track ends. The player that
/// plays keeps it, as only it is sure to be awake then (offline, behind a locked screen).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Sleep {
    #[serde(default)]
    pub at_ms: Option<u64>,
    #[serde(default)]
    pub end_of_track: bool,
}

/// A report's `radio`: Radio's switch, or (an app from before the amendment) its Autoplay as the radio was then.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ReportedRadio {
    On(bool),
    Old(RadioState),
}

/// A value that is there, null included: `Some(None)` for null, the field's absence stays `None` (its default).
fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

/// A player's radio as apps before the amendment report it: on with its mode and seed, or off. Taken as Autoplay.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RadioState {
    pub on: bool,
    #[serde(default)]
    pub mode: RadioMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seed: Vec<String>,
    /// The crate it plays inside, when it does ("Play as radio").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub within: Option<String>,
}

/// What `play` plays when it is not given tracks; resolved here, so it never reaches the player.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayFrom {
    /// A playlist, by name.
    #[serde(default, skip_serializing)]
    pub playlist: Option<String>,
    /// An album of Cratarr's own library (`al-…`), in its order.
    #[serde(default, skip_serializing)]
    pub album: Option<String>,
    /// The owner's favourite tracks.
    #[serde(default, skip_serializing)]
    pub favourites: bool,
    /// Tracks from the whole library, at random ([`LIBRARY_PICK`] of them unless `limit` says otherwise).
    #[serde(default, skip_serializing)]
    pub library: bool,
    /// At most this many tracks, after shuffling.
    #[serde(default, skip_serializing)]
    pub limit: Option<usize>,
}

/// How many tracks `play {library}` picks when not told.
const LIBRARY_PICK: usize = 100;

/// What can be asked of a player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    /// A playlist, an album, the favourites, the library or tracks, from track `index`, `position_ms` into it; shuffled
    /// first when asked (then from the start). What reaches the player is always `tracks`.
    Play {
        #[serde(flatten)]
        from: PlayFrom,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tracks: Option<Vec<QueueTrack>>,
        #[serde(default)]
        index: usize,
        #[serde(default)]
        position_ms: u64,
        /// Asked: shuffle first. Told: the tracks come shuffled (repeat all shuffles each new round).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        shuffle: bool,
        /// Loaded, not started: a queue given back, paused, to a player that came back without one.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        paused: bool,
    },
    /// Tracks into the queue after entry `after` ("" for first), else at place `at`, else at the end. A player is told
    /// both (decision 0033). With `how` (`next`, `last`) they are added by hand, placed as decision 0033's lanes say.
    Add {
        tracks: Vec<QueueTrack>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<String>,
        #[serde(default, skip_serializing)]
        how: Option<String>,
    },
    /// Entry `entry` (or the one at place `from`) to after entry `after` ("" for first), or to place `to`.
    Move {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        entry: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<String>,
    },
    /// Everything after the playing track replaced by these; the playing track plays on.
    Upcoming {
        tracks: Vec<QueueTrack>,
        /// Whether they come shuffled now, when that changed (the shuffle switch).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shuffled: Option<bool>,
    },
    /// Shuffle on or off (decision 0033's second order); a player is told the new upcoming tracks.
    Shuffle { on: bool },
    /// Repeat off, one (the playing track) or all (the album or playlist), decision 0033 amended.
    Repeat { mode: Repeat },
    /// Radio on or off (decision 0036, amended): while on, the player inserts `count` tracks like each own track
    /// (`by` the track, its artist or its era) right after it; off, or another `by` or `count` while on, takes the
    /// inserts to come out. Left out: as it was. With a mode, seed or bound it is the command from before the
    /// amendment, taken as Autoplay during the change-over.
    Radio {
        on: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        by: Option<RadioBy>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count: Option<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<RadioMode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seed: Option<Vec<String>>,
        /// A crate to play inside (its id); "" plays outside any again; left out: as it was.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        within: Option<String>,
    },
    /// Autoplay in a mode, or `stop` (decision 0036, amended): the player appends its tracks when the queue runs out.
    /// What is left out stays as it was; `within` "" plays outside any crate again. On turns repeat all off.
    Autoplay {
        mode: AutoplayMode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        albums: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        restart: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seed: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        within: Option<String>,
    },
    /// Radio's and Autoplay's picks leave the owner's favourites out, or not (decision 0036, amended). A change takes
    /// their picks still to come out.
    SkipFavourites { on: bool },
    /// The sleep timer: pause at `at_ms` (epoch milliseconds) or when the playing track ends; neither clears it.
    Sleep {
        #[serde(default)]
        at_ms: Option<u64>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        end_of_track: bool,
    },
    /// Entry `entry` (or the one at place `index`) out.
    Remove {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        entry: Option<String>,
    },
    /// To entry `entry` (or the one at place `index`), from its start.
    Jump {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        entry: Option<String>,
    },
    Pause,
    Resume,
    Toggle,
    Stop,
    Next,
    Previous,
    /// To `position_ms`, or `percent` (0 to 100) of the track. What reaches the player is always `position_ms`.
    Seek {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        position_ms: Option<u64>,
        #[serde(default, skip_serializing)]
        percent: Option<f64>,
    },
    /// 0 to 100.
    Volume { level: u8 },
    /// Quieter, for an announcement; `Unduck` brings it back.
    Duck,
    Unduck,
    /// The queue and position move to player `to`, and this one stops. Handled here; never sent to a player.
    Handover { to: String },
    /// Sync play, step 1 (decision 0037): what it plays this moment, answered at once (`POST .../sample`).
    Sample { id: u64 },
    /// Sync play (decision 0037): this player plays with `to`'s group, which `to` leads when it plays in none. Handled
    /// here; never sent to a player.
    Join { to: String },
    /// Sync play: this player plays alone again; a leader's music goes on with the others. Handled here.
    Leave,
    /// Sync play, to a player: the group it plays in, by its leader (itself: it leads; none: it plays alone), and where
    /// the group's sound is (Snapcast's port on Cratarr's host, and its buffer). Never taken from outside.
    Group {
        leader: Option<String>,
        /// Where Snapcast's port is, when not on the host the player reaches Cratarr on.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        host: Option<String>,
        port: u16,
        buffer_ms: u32,
    },
}

impl Command {
    /// Play these tracks, as a player is told to.
    pub fn play(tracks: Vec<QueueTrack>, index: usize, position_ms: u64) -> Self {
        Command::Play { from: PlayFrom::default(), tracks: Some(tracks), index, position_ms, shuffle: false, paused: false }
    }
}

/// Cratarr's own ids of the playing track (decision 0030): what Home Assistant's library calls take.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TrackIds {
    pub track_id: Option<String>,
    pub album_id: Option<String>,
    pub album_artist: Option<String>,
}

/// Autoplay as the player view shows it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct AutoplayView {
    pub mode: AutoMode,
    pub albums: bool,
    pub restart: bool,
}

/// A player as Home Assistant and the pages see it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlayerView {
    pub name: String,
    /// Connected now and heard from lately: commands reach it.
    pub online: bool,
    /// `playing`, `paused` or `stopped` (nothing loaded).
    pub state: &'static str,
    /// When it was last heard from (epoch seconds).
    pub seen: u64,
    pub playing: bool,
    pub index: i64,
    /// Now: counted on from the last report while it plays.
    pub position_ms: u64,
    /// When `position_ms` was so (epoch ms): from then on it moves with the music while `playing`.
    pub position_at: u64,
    pub duration_ms: Option<u64>,
    pub volume: u8,
    pub ducked: bool,
    pub queue_length: usize,
    /// Off, one or all (decision 0033, amended).
    pub repeat: Repeat,
    /// What comes of the album or playlist plays out of its own order.
    pub shuffled: bool,
    /// Its sleep timer, while one is set.
    pub sleep: Option<Sleep>,
    /// Radio's switch (decision 0036, amended), what its inserts are like and how many follow each own track.
    pub radio: bool,
    pub radio_by: RadioBy,
    pub radio_count: u8,
    /// Radio's and Autoplay's picks leave the owner's favourites out.
    pub skip_favourites: bool,
    /// Autoplay while on: its mode and switches (its seed and bound stay with the player).
    pub autoplay: Option<AutoplayView>,
    /// Goes up whenever the queue changes: ask for the player by name to see the new one.
    pub queue_version: u64,
    /// When something new started in the queue (epoch seconds, 0: not known): not moved by edits or inserts.
    pub queue_started: u64,
    /// The track at `index`.
    pub track: Option<QueueTrack>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// Cratarr's own ids of the track at `index` (and its album artist).
    #[serde(flatten)]
    pub ids: TrackIds,
    /// The queue entry that plays (decision 0033).
    pub current_entry_id: Option<String>,
    /// The group it plays in, by its leader (decision 0037); a player of a group shows what the group plays.
    pub group: Option<String>,
    /// Its speaker delay (ms), decision 0037.
    pub delay_ms: u16,
    /// `app` (the Android app) or `kodi` (a Kodi Cratarr drives): what it can do (a Kodi joins no group).
    pub kind: &'static str,
    /// The whole queue, for one player asked for by name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue: Option<Vec<QueueTrack>>,
}

/// Saved in `players.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayOnSettings {
    /// Addresses the player list is POSTed to on every change (`{"players": [...]}`).
    #[serde(default)]
    pub webhooks: Vec<String>,
    /// Each player's speaker delay (decision 0037): how late its speaker sounds, in ms; in a group it plays that much
    /// earlier. Set with `PUT /api/players/{name}/delay`; kept when the webhooks are saved.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub delays: std::collections::BTreeMap<String, u16>,
    /// Kodis Cratarr plays on (their passwords are never shown: blank keeps the saved one).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kodi: Vec<super::kodi::KodiConfig>,
}

/// The Play On settings as the page sees them: no password, whether one is set, and Cratarr's address on the local
/// network as a Kodi's default `stream_from`.
#[derive(Debug, Clone, Serialize)]
pub struct PlayOnSettingsView {
    pub webhooks: Vec<String>,
    pub kodi: Vec<KodiView>,
    pub stream_default: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KodiView {
    pub name: String,
    pub url: String,
    pub user: String,
    pub password_set: bool,
    pub stream_from: String,
}

/// The most a speaker delay can be (ms): well inside the group's buffer, which snapclient shortens by it.
pub const MAX_DELAY_MS: u16 = 500;

struct Entry {
    /// The open connection, as (its number, where its commands go).
    connection: Option<(u64, mpsc::Sender<String>)>,
    seen: SystemTime,
    /// When it was last heard from, for the lease.
    heard: Instant,
    /// Cratarr's ids of the track that plays, looked up when it changes, with that track (`source`, `id`): shown only
    /// while it is still the one that plays, as a queue changed here first moves on before the player reports.
    ids: (Option<(String, String)>, TrackIds),
    /// Album ids from the tracks' own sources, for tracks queued without one (`source\nid` → album id).
    album_ids: HashMap<String, String>,
    report: Report,
    /// Its queue, held here (decision 0033).
    queue: Queue,
    /// When `report` came.
    at: Instant,
    /// Connected and not yet heard from: its first report says whether it came back without its queue.
    fresh: bool,
    /// The queue's version in the last edit told to the player: what a report must have seen to be taken.
    told: u64,
    /// The leader of the group it plays in (itself when it leads), decision 0037.
    leader: Option<String>,
    /// When it joined its group, counted: the longest-joined leads when the leader leaves.
    joined: u64,
    kind: PlayerKind,
}

/// What a player is: the app; a Kodi (driven by Cratarr, app::kodi); a page in a browser, a player while it is open
/// (decision 0033; 0031 amended): it plays on its own, never in a group (no sync play there).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum PlayerKind {
    #[default]
    App,
    Kodi,
    Browser,
}

impl PlayerKind {
    fn name(self) -> &'static str {
        match self {
            PlayerKind::App => "app",
            PlayerKind::Kodi => "kodi",
            PlayerKind::Browser => "browser",
        }
    }
}

impl Entry {
    fn new() -> Self {
        Self {
            connection: None,
            seen: SystemTime::now(),
            heard: Instant::now(),
            ids: (None, TrackIds::default()),
            album_ids: HashMap::new(),
            report: Report { index: -1, volume: 100, ..Report::default() },
            queue: Queue::default(),
            at: Instant::now(),
            fresh: false,
            told: 0,
            leader: None,
            joined: 0,
            kind: PlayerKind::App,
        }
    }

    /// The place of the entry that plays: found by its entry where the player says it, else as the player counts.
    fn index(&self) -> i64 {
        let found = self.report.entry.as_deref().and_then(|e| self.queue.at(e));
        found.map_or(self.report.index, |i| i as i64)
    }

    fn online(&self) -> bool {
        self.connection.is_some() && self.heard.elapsed() < LEASE
    }

    fn view(&self, name: &str, with_queue: bool) -> PlayerView {
        let r = &self.report;
        let mut position_ms = r.position_ms;
        if r.playing {
            position_ms += self.at.elapsed().as_millis() as u64;
            if let Some(d) = r.duration_ms {
                position_ms = position_ms.min(d);
            }
        }
        let index = self.index();
        let track = usize::try_from(index).ok().and_then(|i| self.queue.tracks.get(i)).cloned().map(|mut t| {
            if t.album_id.is_none() {
                t.album_id = self.album_ids.get(&format!("{}\n{}", t.source, t.id)).cloned();
            }
            t
        });
        PlayerView {
            name: name.to_string(),
            online: self.online(),
            state: if track.is_none() {
                "stopped"
            } else if r.playing {
                "playing"
            } else {
                "paused"
            },
            seen: self.seen.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
            playing: r.playing,
            index,
            position_ms,
            position_at: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
            duration_ms: r.duration_ms,
            volume: r.volume,
            ducked: r.ducked,
            queue_length: self.queue.tracks.len(),
            repeat: self.queue.repeat,
            shuffled: shuffled_order(&self.queue.tracks, index),
            sleep: r.sleep.filter(|s| s.at_ms.is_some() || s.end_of_track),
            radio: self.queue.radio,
            radio_by: self.queue.radio_by,
            radio_count: self.queue.radio_count(),
            skip_favourites: self.queue.skip_favourites,
            autoplay: self.queue.autoplay.as_ref().map(|a| AutoplayView { mode: a.mode, albums: a.albums, restart: a.restart }),
            queue_version: self.queue.version,
            queue_started: self.queue.started,
            title: track.as_ref().map(|t| t.name.clone()),
            artist: track.as_ref().and_then(|t| t.artist.clone()),
            album: track.as_ref().and_then(|t| t.album.clone()),
            ids: if self.ids.0 == key(track.as_ref()) { self.ids.1.clone() } else { TrackIds::default() },
            current_entry_id: track.as_ref().map(|t| t.entry.clone()).filter(|e| !e.is_empty()),
            group: self.leader.clone(),
            delay_ms: 0,
            kind: self.kind.name(),
            track,
            queue: with_queue.then(|| self.queue.tracks.clone()),
        }
    }
}

/// A player as seen: a player of a group that does not lead it shows what the group plays, with its own name, reach
/// and volume (decision 0037).
fn view_in(entries: &HashMap<String, Entry>, name: &str, with_queue: bool) -> Option<PlayerView> {
    let e = entries.get(name)?;
    let own = e.view(name, with_queue);
    let leader = e.leader.as_deref().filter(|l| *l != name).and_then(|l| Some((l, entries.get(l)?)));
    Some(match leader {
        Some((l, le)) => PlayerView {
            name: own.name,
            online: own.online,
            seen: own.seen,
            volume: own.volume,
            ducked: own.ducked,
            // What the device is (a Kodi stays one, so it is never made a leader).
            kind: own.kind,
            ..le.view(l, with_queue)
        },
        None => own,
    })
}

/// The one PlayOn, for the Cratarr source's players (decision 0032), which plugins cannot be handed.
static INSTANCE: std::sync::OnceLock<Arc<PlayOn>> = std::sync::OnceLock::new();

pub struct PlayOn {
    entries: Mutex<HashMap<String, Entry>>,
    changed: broadcast::Sender<()>,
    connections: AtomicU64,
    /// Numbers the webhook POSTs: a receiver drops one older than the last it took.
    seq: AtomicU64,
    lib: Arc<super::Library>,
    settings: Mutex<PlayOnSettings>,
    file: Option<PathBuf>,
    /// The queues, saved after every change (`queues.json`), so a player that comes back without one gets it back.
    queues_file: Option<PathBuf>,
    save: tokio::sync::Notify,
    playlists: Arc<PlaylistService>,
    /// Sync play's measurements, waiting for the players' samples (decision 0037, step 1).
    samples: super::together::Samples,
    /// Where groups' sound is (Snapcast's port, its buffer, the host when another), once Cratarr serves it (0037).
    sync_play: Mutex<Option<(u16, u32, Option<String>)>>,
    /// Counts joins, for who joined longest ago.
    joins: AtomicU64,
    /// The Kodi drivers running (stopped when the settings change).
    kodis: Mutex<Vec<tokio::task::AbortHandle>>,
    /// Cratarr's address on the local network (a Kodi's default `stream_from`).
    lan_base: Mutex<Option<String>>,
}

/// A player's queue as saved, with where it was in it and whether it played.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Saved {
    #[serde(flatten)]
    queue: Queue,
    #[serde(default)]
    entry: Option<String>,
    #[serde(default)]
    position_ms: u64,
    #[serde(default)]
    playing: bool,
}

impl PlayOn {
    pub fn new(lib: Arc<super::Library>, playlists: Arc<PlaylistService>, data_dir: Option<&str>) -> Arc<Self> {
        let file = data_dir.map(|d| PathBuf::from(d).join("players.json"));
        let settings = file.as_deref().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let queues_file = data_dir.map(|d| PathBuf::from(d).join("queues.json"));
        let saved: HashMap<String, Saved> =
            queues_file.as_deref().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        // The players known before a restart, offline until they connect, with their queues.
        let entries = saved
            .into_iter()
            .map(|(name, s)| {
                let mut e = Entry::new();
                e.report.index = s.entry.as_deref().and_then(|x| s.queue.at(x)).map_or(-1, |i| i as i64);
                e.report.entry = s.entry;
                e.report.position_ms = s.position_ms;
                e.report.playing = s.playing;
                e.queue = s.queue;
                (name, e)
            })
            .collect();
        let me = Arc::new(Self {
            entries: Mutex::new(entries),
            changed: broadcast::channel(16).0,
            connections: AtomicU64::new(0),
            seq: AtomicU64::new(0),
            lib,
            settings: Mutex::new(settings),
            file,
            queues_file,
            save: tokio::sync::Notify::new(),
            playlists,
            samples: super::together::Samples::default(),
            sync_play: Mutex::new(None),
            joins: AtomicU64::new(1),
            kodis: Mutex::new(Vec::new()),
            lan_base: Mutex::new(None),
        });
        me.clone().tell_webhooks();
        me.clone().watch_leases();
        me.clone().keep_queues();
        me.start_kodis();
        let _ = INSTANCE.set(me.clone());
        me
    }

    /// Cratarr's address on the local network, as players there reach it (a Kodi streams from it).
    pub fn set_lan_base(self: &Arc<Self>, base: Option<String>) {
        *self.lan_base.lock().unwrap_or_else(|p| p.into_inner()) = base;
        self.start_kodis();
    }

    /// A driver for each Kodi of the settings (the ones before stopped).
    fn start_kodis(self: &Arc<Self>) {
        let base = self.lan_base.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let mut running = self.kodis.lock().unwrap_or_else(|p| p.into_inner());
        for h in running.drain(..) {
            h.abort();
        }
        for mut cfg in self.settings().kodi {
            if cfg.stream_from.trim().is_empty() {
                cfg.stream_from = base.clone().unwrap_or_default();
            }
            if cfg.name.trim().is_empty() || cfg.url.trim().is_empty() || cfg.stream_from.trim().is_empty() {
                continue;
            }
            running.push(tokio::spawn(super::kodi::drive(Arc::downgrade(self), cfg)).abort_handle());
        }
    }

    /// A Kodi's driver joins as a player (`app::kodi`).
    pub(super) fn connect_kodi(self: &Arc<Self>, name: &str) -> Result<Commands, ToggleError> {
        let c = self.connect(name)?;
        if let Some(e) = self.entries().get_mut(name) {
            e.kind = PlayerKind::Kodi;
        }
        Ok(c)
    }

    /// A page in a browser joins as a player while it is open (decision 0033; 0031 amended).
    pub fn connect_browser(self: &Arc<Self>, name: &str) -> Result<Commands, ToggleError> {
        let c = self.connect(name)?;
        if let Some(e) = self.entries().get_mut(name) {
            e.kind = PlayerKind::Browser;
        }
        Ok(c)
    }

    /// The Play On settings as the page sees them.
    pub fn settings_view(&self) -> PlayOnSettingsView {
        let s = self.settings();
        PlayOnSettingsView {
            webhooks: s.webhooks,
            kodi: s
                .kodi
                .into_iter()
                .map(|k| KodiView { password_set: !k.password.is_empty(), name: k.name, url: k.url, user: k.user, stream_from: k.stream_from })
                .collect(),
            stream_default: self.lan_base.lock().unwrap_or_else(|p| p.into_inner()).clone(),
        }
    }

    /// Sync play, step 1 (decision 0037): every online player asked what it plays this moment, and the answers
    /// compared at one moment of Cratarr's time.
    pub async fn measure(&self) -> super::together::Measure {
        let id = self.samples.open();
        let online: Vec<String> = self.entries().iter().filter(|(_, e)| e.online()).map(|(n, _)| n.clone()).collect();
        let asked: Vec<String> = online.into_iter().filter(|n| self.send(n, &Command::Sample { id }).is_ok()).collect();
        self.samples.wait(id, &asked).await
    }

    /// A player's answer to a measurement.
    pub fn sample(&self, name: &str, s: super::together::Sample) -> Result<(), ToggleError> {
        valid_name(name)?;
        self.samples.answer(name, s);
        Ok(())
    }

    /// The PlayOn of this Cratarr, once it is open.
    pub fn instance() -> Option<Arc<Self>> {
        INSTANCE.get().cloned()
    }

    /// The player Cratarr itself follows (its own source, 0032): of those online with something loaded, the one
    /// playing, else the one heard from last; else any online one. With its queue. `None` when none is online.
    pub fn followed(&self) -> Option<PlayerView> {
        let entries = self.entries();
        let online = entries.iter().filter(|(_, e)| e.online());
        let best = online
            .clone()
            .filter(|(_, e)| e.index() >= 0)
            .max_by_key(|(_, e)| (e.report.playing, std::cmp::Reverse(e.at.elapsed())))
            .or_else(|| online.max_by_key(|(_, e)| std::cmp::Reverse(e.heard.elapsed())));
        best.and_then(|(n, _)| view_in(&entries, n, true))
    }

    /// A short summary of what the players do (track, playing, queue), compared to notice a change cheaply.
    pub fn summary(&self) -> String {
        self.list()
            .iter()
            .map(|p| format!("{}|{}|{}|{}|{}", p.name, p.online, p.index, p.playing, p.queue_version))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Cratarr's ids of a player's track, from its own library (0030); `None` parts when that does not know it.
    fn ids_of(&self, t: Option<&QueueTrack>) -> TrackIds {
        let (Some(t), Some(served)) = (t, crate::serve::instance()) else { return TrackIds::default() };
        let library = &served.library;
        let Some(track) = self.lib.catalogue().track_of_source(&t.source, &t.id) else { return TrackIds::default() };
        match library.song_ids(track) {
            Ok(Some(ids)) => TrackIds { track_id: Some(ids.song_id), album_id: Some(ids.album_id), album_artist: Some(ids.album_artist) },
            _ => TrackIds { track_id: Some(crate::serve::ids::song(track)), ..TrackIds::default() },
        }
    }

    /// Asks the track's own source for its album's id, and tells of the change once it is known.
    fn find_album_id(self: &Arc<Self>, name: &str, t: QueueTrack) {
        let Some(source) = self.lib.connected_sources().into_iter().find(|(id, _)| *id == t.source).map(|(_, s)| s) else { return };
        let (me, name) = (Arc::downgrade(self), name.to_string());
        tokio::spawn(async move {
            let Ok(found) = source.track(&t.id).await else { return };
            let (Some(album), Some(me)) = (found.album_id, me.upgrade()) else { return };
            if let Some(e) = me.entries().get_mut(&name) {
                e.album_ids.insert(format!("{}\n{}", t.source, t.id), album);
            }
            me.changed();
        });
    }

    /// A player whose lease runs out goes offline without a word from it: looked at every 15 s.
    fn watch_leases(self: Arc<Self>) {
        let me = Arc::downgrade(&self);
        drop(self);
        tokio::spawn(async move {
            let mut was: HashMap<String, bool> = HashMap::new();
            loop {
                tokio::time::sleep(Duration::from_secs(15)).await;
                let Some(me) = me.upgrade() else { return };
                let now: HashMap<String, bool> = me.entries().iter().map(|(n, e)| (n.clone(), e.online())).collect();
                if now.iter().any(|(n, on)| was.get(n).is_some_and(|w| w != on)) {
                    me.changed();
                }
                was = now;
                // Gone (its lease over) leaves its group, which plays on; back, it does not join again by itself.
                let gone: Vec<String> = me.entries().iter().filter(|(_, e)| e.leader.is_some() && e.heard.elapsed() >= LEASE).map(|(n, _)| n.clone()).collect();
                for n in gone {
                    tracing::info!("player {n:?} is gone: out of its group");
                    let _ = me.leave(&n).await;
                }
            }
        });
    }

    fn entries(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
        self.entries.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn changed(&self) {
        let _ = self.changed.send(());
        self.save.notify_one();
    }

    /// Saves the queues after changes (those close together as one), in the background.
    fn keep_queues(self: Arc<Self>) {
        let Some(file) = self.queues_file.clone() else { return };
        let me = Arc::downgrade(&self);
        drop(self);
        tokio::spawn(async move {
            let mut last = String::new();
            loop {
                let Some(notified) = me.upgrade() else { return };
                notified.save.notified().await;
                drop(notified);
                tokio::time::sleep(Duration::from_secs(1)).await;
                let Some(me) = me.upgrade() else { return };
                let saved: std::collections::BTreeMap<String, Saved> = me
                    .entries()
                    .iter()
                    .map(|(n, e)| {
                        let v = e.view(n, false);
                        (n.clone(), Saved { queue: e.queue.clone(), entry: v.current_entry_id, position_ms: v.position_ms, playing: v.playing })
                    })
                    .collect();
                drop(me);
                let Ok(body) = serde_json::to_string(&saved) else { continue };
                if body == last {
                    continue;
                }
                let (f, b) = (file.clone(), body.clone());
                match tokio::task::spawn_blocking(move || write_private(&f, &b)).await {
                    Ok(Ok(())) => last = body,
                    Ok(Err(e)) => tracing::warn!("Play On: cannot save the queues: {e:#}"),
                    Err(e) => tracing::warn!("Play On: cannot save the queues: {e}"),
                }
            }
        });
    }

    /// Told on every change of a player (the pages' live channel).
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.changed.subscribe()
    }

    /// Every player Cratarr has heard of since it started, by name.
    pub fn list(&self) -> Vec<PlayerView> {
        let entries = self.entries();
        let mut out: Vec<PlayerView> = entries.keys().filter_map(|n| view_in(&entries, n, false)).collect();
        drop(entries);
        let delays = self.settings().delays;
        for v in &mut out {
            v.delay_ms = delays.get(&v.name).copied().unwrap_or(0);
        }
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        out
    }

    /// One player, with its queue.
    pub fn get(&self, name: &str) -> Option<PlayerView> {
        let v = view_in(&self.entries(), name, true);
        v.map(|v| PlayerView { delay_ms: self.delay_of(name), ..v })
    }

    /// A player connects: the commands for it come out of the returned stream, which closes the connection when it
    /// is dropped (the player went away). A second connection under the same name replaces the first.
    pub fn connect(self: &Arc<Self>, name: &str) -> Result<Commands, ToggleError> {
        valid_name(name)?;
        let (tx, rx) = mpsc::channel(16);
        let id = self.connections.fetch_add(1, Ordering::Relaxed);
        {
            let mut entries = self.entries();
            let e = entries.entry(name.to_string()).or_insert_with(Entry::new);
            e.connection = Some((id, tx));
            e.seen = SystemTime::now();
            e.heard = Instant::now();
            e.fresh = true;
        }
        // Its group as Cratarr knows it (none, after it was gone): what it played with before it went is set straight.
        self.tell_group(name);
        tracing::info!("player {name:?} connected");
        self.changed();
        Ok(Commands { rx, play_on: self.clone(), name: name.to_string(), id })
    }

    fn disconnected(&self, name: &str, id: u64) {
        let mut entries = self.entries();
        let Some(e) = entries.get_mut(name) else { return };
        if e.connection.as_ref().is_some_and(|(c, _)| *c == id) {
            e.connection = None;
            drop(entries);
            tracing::info!("player {name:?} disconnected");
            self.changed();
        }
    }

    /// What a player says it plays.
    pub fn report(self: &Arc<Self>, name: &str, report: Report) -> Result<(), ToggleError> {
        valid_name(name)?;
        let mut entries = self.entries();
        let e = entries.entry(name.to_string()).or_insert_with(Entry::new);
        let before = e.view(name, false);
        e.seen = SystemTime::now();
        e.heard = Instant::now();
        // Back without a queue (the app was updated, or killed): it gets its queue back where it was, playing on if it
        // played (the owner's wish, 2026-10-15), else paused.
        let came_back_empty = std::mem::take(&mut e.fresh) && report.index < 0 && !e.queue.tracks.is_empty() && e.kind != PlayerKind::Kodi;
        let give_back = came_back_empty.then(|| {
            let (index, position_ms, paused) = (before.index.max(0) as usize, before.position_ms, !before.playing);
            Command::Play { from: PlayFrom::default(), tracks: Some(e.queue.tracks.clone()), index, position_ms, shuffle: false, paused }
        });
        if give_back.is_some() {
            e.at = Instant::now();
        } else {
            // Taken as the player holds it, unless it was made before Cratarr's latest edit reached the player: that edit
            // is on its way, and the player reports again once it has it (no flip back and forth).
            if report.version.is_none_or(|v| v >= e.told) {
                if let Some(q) = report.queue.clone() {
                    e.queue.adopt(q);
                }
                if let Some(r) = report.repeat {
                    e.queue.repeat = r;
                }
                match &report.radio {
                    Some(ReportedRadio::On(on)) => e.queue.radio = *on,
                    // An app from before the amendment: its radio was Autoplay.
                    Some(ReportedRadio::Old(r)) => {
                        e.queue.autoplay = r.on.then(|| {
                            let (mode, albums, restart) = from_old(Some(r.mode));
                            Autoplay { mode, albums, restart, seed: r.seed.clone(), within: r.within.clone() }
                        })
                    }
                    None => {}
                }
                if let Some(b) = report.radio_by {
                    e.queue.radio_by = b;
                }
                if let Some(n) = report.radio_count {
                    e.queue.radio_count = Some(n);
                }
                if let Some(on) = report.skip_favourites {
                    e.queue.skip_favourites = on;
                }
                if let Some(a) = &report.autoplay {
                    e.queue.autoplay = a.clone();
                }
            }
            e.report = Report { queue: None, ..report };
            e.at = Instant::now();
        }
        let track = e.view(name, false).track;
        let new_track = key(track.as_ref()) != e.ids.0;
        drop(entries);
        if let Some(c) = give_back {
            tracing::info!("player {name:?} came back without its queue: given back");
            let _ = self.send(name, &c);
        }
        // A new track: its ids, looked up outside the lock; its album's id from its own source when it came without one
        // (queued by a page, handed over): Home Assistant finds "this album" by it.
        if new_track {
            let ids = self.ids_of(track.as_ref());
            if let Some(e) = self.entries().get_mut(name) {
                e.ids = (key(track.as_ref()), ids);
            }
            if let Some(t) = track.as_ref().filter(|t| t.album_id.is_none()) {
                self.find_album_id(name, t.clone());
            }
        }
        let after = match self.entries().get(name) {
            Some(e) => e.view(name, false),
            None => return Ok(()),
        };
        // A position that moves on as counted is no news; a jump (a seek) is.
        let jumped = before.position_ms.abs_diff(after.position_ms) > 2_000;
        let still = |v: &PlayerView| PlayerView { position_ms: 0, position_at: 0, seen: 0, ..v.clone() };
        if still(&before) != still(&after) || jumped {
            self.changed();
        }
        Ok(())
    }

    /// Asks player `name` to do something.
    pub async fn command(&self, name: &str, command: Command) -> Result<(), ToggleError> {
        // A command to any player of a group is the group's, played by its leader; its volume stays its own.
        let own = matches!(command, Command::Volume { .. } | Command::Duck | Command::Unduck | Command::Sample { .. } | Command::Join { .. } | Command::Leave);
        let leader = if own { None } else { self.leader_of(name) };
        let name = leader.as_deref().unwrap_or(name);
        let command = match command {
            Command::Join { to } => return self.join(name, &to).await,
            Command::Leave => return self.leave(name).await,
            Command::Group { .. } => return Err(ToggleError::Refused("A group is made with join and leave.".into())),
            // Played on one player: the group ends (decision 0037).
            Command::Handover { to } => {
                self.dissolve(name);
                return self.handover(name, &to).await;
            }
            Command::Play { from, tracks, mut index, mut position_ms, shuffle, paused } => {
                let picked = match &from.playlist {
                    Some(p) => Some(self.playlist_tracks(p).await?),
                    None => own_tracks(&from).await?,
                };
                let Some(mut tracks) = picked.or(tracks) else {
                    return Err(ToggleError::Refused("Play what? Give a playlist, an album, the favourites, the library or tracks.".into()));
                };
                if tracks.is_empty() {
                    return Err(ToggleError::Refused("There is nothing there that can be played.".into()));
                }
                // The album's or playlist's own order kept on its tracks, for turning shuffle off later.
                for (i, t) in tracks.iter_mut().enumerate().filter(|(_, t)| t.lane.is_none() && t.n.is_none()) {
                    t.n = u32::try_from(i).ok();
                }
                if shuffle {
                    shuffled(&mut tracks);
                    (index, position_ms) = (0, 0);
                }
                if let Some(n) = from.limit {
                    tracks.truncate(n.max(1));
                }
                let tracks = Some(self.named(tracks).await);
                Command::Play { from: PlayFrom::default(), tracks, index, position_ms, shuffle, paused }
            }
            Command::Add { tracks, at, after, how } => Command::Add { tracks: self.named(tracks).await, at, after, how },
            Command::Seek { position_ms: None, percent: Some(p) } => {
                let duration = self.entries().get(name).and_then(|e| e.report.duration_ms);
                let Some(d) = duration else { return Err(ToggleError::Refused("The track's length is not known yet.".into())) };
                Command::Seek { position_ms: Some((d as f64 * p.clamp(0.0, 100.0) / 100.0) as u64), percent: None }
            }
            Command::Seek { position_ms: None, percent: None } => return Err(ToggleError::Refused("Seek where?".into())),
            other => other,
        };
        // The queue is edited here first (decision 0033), and the player told the edit by entry.
        let mut entries = self.entries();
        let Some(e) = entries.get_mut(name) else { return Err(ToggleError::Refused(format!("No player is called {name}."))) };
        if !e.online() {
            return Err(ToggleError::Refused(format!("{name} is not connected.")));
        }
        let mut queue = e.queue.clone();
        let told = queue.edit_at(command, usize::try_from(e.index()).ok())?;
        Self::tell(e, name, &told, queue.version)?;
        let edited = queue != e.queue;
        if edited {
            e.told = queue.version;
        }
        e.queue = queue;
        drop(entries);
        if edited {
            self.changed();
        }
        Ok(())
    }

    /// A command to a player as it is, its queue not edited here (a queue given back, a handover's stop).
    fn send(&self, name: &str, command: &Command) -> Result<(), ToggleError> {
        let mut entries = self.entries();
        let Some(e) = entries.get_mut(name) else { return Err(ToggleError::Refused(format!("No player is called {name}."))) };
        let version = e.queue.version;
        // A queue given back replaces the player's: what it reports from before does not count.
        if matches!(command, Command::Play { .. }) {
            e.told = version;
        }
        Self::tell(e, name, command, version)
    }

    /// Sends a command down a player's connection, with the version of the queue it leaves the player at.
    fn tell(e: &mut Entry, name: &str, command: &Command, version: u64) -> Result<(), ToggleError> {
        let mut json = serde_json::to_value(command).map_err(anyhow::Error::from)?;
        json["version"] = version.into();
        let Some((_, tx)) = &e.connection else { return Err(ToggleError::Refused(format!("{name} is not connected."))) };
        tx.try_send(json.to_string()).map_err(|_| ToggleError::Refused(format!("{name} does not take commands now.")))
    }

    /// The queue and position of `from` go to `to`, which plays them, with their entries; `from` stops (decision 0033).
    async fn handover(&self, from: &str, to: &str) -> Result<(), ToggleError> {
        let (tracks, index, position_ms) = {
            let entries = self.entries();
            let Some(e) = entries.get(from) else { return Err(ToggleError::Refused(format!("No player is called {from}."))) };
            let v = e.view(from, false);
            (e.queue.tracks.clone(), v.index.max(0) as usize, v.position_ms)
        };
        if tracks.is_empty() {
            return Err(ToggleError::Refused(format!("{from} has nothing to hand over.")));
        }
        let (repeat, sleep, radio, autoplay) = self
            .entries()
            .get(from)
            .map(|e| {
                let radio = (e.queue.radio, e.queue.radio_by, e.queue.radio_count(), e.queue.skip_favourites);
                (e.queue.repeat, e.report.sleep, radio, e.queue.autoplay.clone())
            })
            .unwrap_or_default();
        Box::pin(self.command(to, Command::play(tracks, index, position_ms))).await?;
        // Repeat moves with the queue (decision 0033, amended), repeat one too; a sleep timer moves with the music; Radio
        // with its choice and count, and Autoplay too, with its seed and bound (decision 0036, amended). Autoplay last
        // (on turns repeat all off, and both came so).
        let _ = Box::pin(self.command(to, Command::Repeat { mode: repeat })).await;
        let (on, by, count, skip) = radio;
        let radio = Command::Radio { on, mode: None, seed: None, within: None, by: Some(by), count: Some(count.max(1)) };
        let _ = Box::pin(self.command(to, radio)).await;
        let _ = Box::pin(self.command(to, Command::SkipFavourites { on: skip })).await;
        let auto = match autoplay {
            Some(a) => Command::Autoplay {
                mode: AutoplayMode::of(Some(a.mode)),
                albums: Some(a.albums),
                restart: Some(a.restart),
                seed: Some(a.seed),
                within: Some(a.within.unwrap_or_default()),
            },
            None => Command::Autoplay { mode: AutoplayMode::Stop, albums: None, restart: None, seed: None, within: None },
        };
        let _ = Box::pin(self.command(to, auto)).await;
        if let Some(s) = sleep.filter(|s| s.at_ms.is_some() || s.end_of_track) {
            let _ = Box::pin(self.command(to, Command::Sleep { at_ms: s.at_ms, end_of_track: s.end_of_track })).await;
        }
        // The music is on its way there; this one stopping is no reason to fail the handover.
        let _ = Box::pin(self.command(from, Command::Stop)).await;
        Ok(())
    }

    /// Cratarr serves groups' sound (decision 0037): on Snapcast's `port` (on `host`, when the players reach it on
    /// another host than Cratarr's pages), played `buffer_ms` after it is stamped.
    pub fn serve_sync_play(&self, port: u16, buffer_ms: u32, host: Option<String>) {
        *self.sync_play.lock().unwrap_or_else(|p| p.into_inner()) = Some((port, buffer_ms, host));
    }

    fn sync_play(&self) -> Option<(u16, u32, Option<String>)> {
        self.sync_play.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// The leader of the group `name` plays in (itself when it leads); `None` when it plays alone.
    pub fn leader_of(&self, name: &str) -> Option<String> {
        self.entries().get(name).and_then(|e| e.leader.clone())
    }

    /// The players of `leader`'s group, the longest-joined first (the leader among them).
    fn group(&self, leader: &str) -> Vec<String> {
        let entries = self.entries();
        let mut players: Vec<(u64, String)> =
            entries.iter().filter(|(_, e)| e.leader.as_deref() == Some(leader)).map(|(n, e)| (e.joined, n.clone())).collect();
        players.sort();
        players.into_iter().map(|(_, n)| n).collect()
    }

    /// Tells a player the group it plays in (none: alone), as Cratarr knows it.
    fn tell_group(&self, name: &str) {
        let (port, buffer_ms, host) = self.sync_play().unwrap_or((crate::snapcast::PORT, crate::snapcast::BUFFER_MS, None));
        let leader = self.leader_of(name);
        let _ = self.send(name, &Command::Group { leader, host, port, buffer_ms });
    }

    /// `name` plays with `to`'s group, which `to` leads when it plays in none (decision 0037). Out of any group it
    /// played in first.
    async fn join(&self, name: &str, to: &str) -> Result<(), ToggleError> {
        if self.sync_play().is_none() {
            return Err(ToggleError::Refused("Sync play is not served here.".into()));
        }
        let leader = {
            let entries = self.entries();
            for n in [name, to] {
                let Some(e) = entries.get(n) else { return Err(ToggleError::Refused(format!("No player is called {n}."))) };
                if !e.online() {
                    return Err(ToggleError::Refused(format!("{n} is not connected.")));
                }
                // A page in a browser has no sync play: it plays on its own.
                if e.kind == PlayerKind::Browser {
                    return Err(ToggleError::Refused(format!("{n} plays in a browser, which cannot play in a group.")));
                }
            }
            let leader = entries.get(to).and_then(|e| e.leader.clone()).unwrap_or_else(|| to.to_string());
            // A Kodi follows, never leads (the owner): its sound cannot be sent on.
            if entries.get(&leader).is_some_and(|e| e.kind != PlayerKind::App) {
                return Err(ToggleError::Refused(format!("{leader} cannot lead a group: play on another player and add {leader} to it.")));
            }
            leader
        };
        if name == leader || self.leader_of(name).as_deref() == Some(leader.as_str()) {
            return Ok(());
        }
        if self.leader_of(name).is_some() {
            self.leave(name).await?;
        }
        let newly_leads = {
            let mut entries = self.entries();
            let n = self.joins.fetch_add(2, Ordering::Relaxed);
            let newly = entries.get_mut(&leader).filter(|l| l.leader.is_none()).map(|l| {
                l.leader = Some(leader.clone());
                l.joined = n;
            });
            if let Some(e) = entries.get_mut(name) {
                e.leader = Some(leader.clone());
                e.joined = n + 1;
            }
            newly.is_some()
        };
        tracing::info!("player {name:?} plays with {leader:?}");
        if newly_leads {
            self.tell_group(&leader);
        }
        self.tell_group(name);
        self.changed();
        Ok(())
    }

    /// `name` plays alone again. A leader's music goes on with the others, the longest-joined leading (a handover, as
    /// decision 0033's); the last one left plays alone.
    async fn leave(&self, name: &str) -> Result<(), ToggleError> {
        let Some(leader) = self.leader_of(name) else { return Ok(()) };
        if let Some(e) = self.entries().get_mut(name) {
            e.leader = None;
        }
        self.tell_group(name);
        let rest = self.group(&leader);
        tracing::info!("player {name:?} plays alone again");
        if leader == name {
            // The longest-joined of the app's players leads on; with none (Kodis only), the first plays alone.
            let apps: Vec<String> = {
                let entries = self.entries();
                rest.iter().filter(|n| entries.get(*n).is_some_and(|e| e.kind == PlayerKind::App)).cloned().collect()
            };
            let next = apps.first().or(rest.first()).cloned();
            if let Some(next) = next.as_ref() {
                let alone = rest.len() == 1 || apps.is_empty();
                {
                    let mut entries = self.entries();
                    for n in &rest {
                        if let Some(e) = entries.get_mut(n) {
                            e.leader = (!alone).then(|| next.clone());
                        }
                    }
                }
                for n in &rest {
                    self.tell_group(n);
                }
                let _ = self.handover(name, next).await;
            }
        } else if rest.len() == 1 {
            // Only the leader left: it plays alone.
            if let Some(e) = self.entries().get_mut(&leader) {
                e.leader = None;
            }
            self.tell_group(&leader);
        }
        self.changed();
        Ok(())
    }

    /// The group `name` plays in ends: everyone in it plays alone (its music stays with the leader).
    fn dissolve(&self, name: &str) {
        let Some(leader) = self.leader_of(name) else { return };
        let players = self.group(&leader);
        {
            let mut entries = self.entries();
            for n in &players {
                if let Some(e) = entries.get_mut(n) {
                    e.leader = None;
                }
            }
        }
        for n in &players {
            self.tell_group(n);
        }
        self.changed();
    }

    /// Tracks sent by their ids only (a page sends a playlist's ids and looks their names up later) get their title,
    /// artist and album from the catalogue, so the player, the pages and Home Assistant all show them.
    async fn named(&self, tracks: Vec<QueueTrack>) -> Vec<QueueTrack> {
        if tracks.iter().all(|t| !t.name.is_empty()) {
            return tracks;
        }
        let (cat, sent) = (self.lib.catalogue(), tracks.clone());
        let named = tokio::task::spawn_blocking(move || {
            let mut tracks = tracks;
            let mut by_source: HashMap<String, Vec<usize>> = HashMap::new();
            for (i, t) in tracks.iter().enumerate().filter(|(_, t)| t.name.is_empty()) {
                by_source.entry(t.source.clone()).or_default().push(i);
            }
            for (source, at) in by_source {
                let ids: Vec<String> = at.iter().map(|&i| tracks[i].id.clone()).collect();
                let Ok(rows) = cat.source_rows(&source, &ids) else { continue };
                for (&i, r) in at.iter().zip(rows) {
                    let t = &mut tracks[i];
                    t.name = r.title.unwrap_or_default();
                    t.artist = t.artist.take().or(r.artist);
                    t.album = t.album.take().or(r.album);
                }
            }
            tracks
        })
        .await;
        named.unwrap_or(sent)
    }

    /// A playlist's playable tracks, as players hold them.
    async fn playlist_tracks(&self, playlist: &str) -> Result<Vec<QueueTrack>, ToggleError> {
        // By its name, regardless of capitals when no playlist has the very name ("Restaurant de Lift" for "Restaurant
        // De Lift"), as the spoken names are found.
        let list = self.playlists.view(None).await?;
        let name = match list.iter().find(|p| p.id == playlist) {
            Some(p) => p.id.clone(),
            None => {
                let want = playlist.to_lowercase();
                let mut like = list.iter().filter(|p| p.id.to_lowercase() == want);
                match (like.next(), like.next()) {
                    (Some(p), None) => p.id.clone(),
                    _ => playlist.to_string(),
                }
            }
        };
        let (source, rows) = self.playlists.rows(&name, None).await?;
        Ok(rows
            .into_iter()
            .filter(|r| !r.missing)
            .filter_map(|r| {
                Some(QueueTrack {
                    source: source.clone(),
                    id: r.id?,
                    name: r.name.unwrap_or_default(),
                    artist: r.artist,
                    album: r.album,
                    duration_secs: r.duration_secs,
                    ..QueueTrack::default()
                })
            })
            .collect())
    }

    pub fn settings(&self) -> PlayOnSettings {
        self.settings.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub async fn save_settings(self: &Arc<Self>, draft: PlayOnSettings) -> Result<PlayOnSettingsView, ToggleError> {
        let Some(file) = self.file.clone() else { return Err(ToggleError::SettingsReadOnly) };
        let mut webhooks = Vec::new();
        for w in draft.webhooks.iter().map(|w| w.trim()).filter(|w| !w.is_empty()) {
            if !(w.starts_with("http://") || w.starts_with("https://")) || reqwest::Url::parse(w).is_err() {
                return Err(ToggleError::BadSettings(format!("{w} is not a web address (http:// or https://)")));
            }
            webhooks.push(w.to_string());
        }
        let before = self.settings();
        let mut kodi: Vec<super::kodi::KodiConfig> = Vec::new();
        for k in draft.kodi {
            let name = k.name.trim().to_string();
            valid_name(&name)?;
            let url = k.url.trim().to_string();
            if !(url.starts_with("http://") || url.starts_with("https://")) || reqwest::Url::parse(&url).is_err() {
                return Err(ToggleError::BadSettings(format!("{url} is not Kodi's web address (http://host:8080)")));
            }
            let stream_from = k.stream_from.trim().trim_end_matches('/').to_string();
            if !stream_from.is_empty() && reqwest::Url::parse(&stream_from).is_err() {
                return Err(ToggleError::BadSettings(format!("{stream_from} is not an address")));
            }
            if kodi.iter().any(|o| o.name == name) {
                return Err(ToggleError::BadSettings(format!("two Kodis are called {name}")));
            }
            let password = if k.password.is_empty() {
                before.kodi.iter().find(|o| o.name == name).map(|o| o.password.clone()).unwrap_or_default()
            } else {
                k.password
            };
            kodi.push(super::kodi::KodiConfig { name, url, user: k.user.trim().to_string(), password, stream_from });
        }
        let saved = PlayOnSettings { webhooks, delays: before.delays, kodi };
        let body = serde_json::to_string_pretty(&saved).map_err(anyhow::Error::from)?;
        tokio::task::spawn_blocking(move || write_private(&file, &body)).await.map_err(|e| anyhow::anyhow!("{e}"))??;
        *self.settings.lock().unwrap_or_else(|p| p.into_inner()) = saved;
        self.start_kodis();
        self.changed();
        Ok(self.settings_view())
    }

    /// A player's speaker delay (ms).
    pub fn delay_of(&self, name: &str) -> u16 {
        self.settings.lock().unwrap_or_else(|p| p.into_inner()).delays.get(name).copied().unwrap_or(0)
    }

    /// Sets a player's speaker delay (0 to [`MAX_DELAY_MS`]); heard at once in a group. Saved in `players.json` when
    /// there is one.
    pub async fn set_delay(&self, name: &str, ms: u16) -> Result<u16, ToggleError> {
        valid_name(name)?;
        // A Kodi's delay is its own buffering too: up to the group's whole buffer.
        let kodi = self.entries().get(name).is_some_and(|e| e.kind == PlayerKind::Kodi);
        let ms = ms.min(if kodi { crate::snapcast::BUFFER_MS as u16 } else { MAX_DELAY_MS });
        let saved = {
            let mut settings = self.settings.lock().unwrap_or_else(|p| p.into_inner());
            if ms == 0 {
                settings.delays.remove(name);
            } else {
                settings.delays.insert(name.to_string(), ms);
            }
            settings.clone()
        };
        if let Some(file) = self.file.clone() {
            let body = serde_json::to_string_pretty(&saved).map_err(anyhow::Error::from)?;
            tokio::task::spawn_blocking(move || write_private(&file, &body)).await.map_err(|e| anyhow::anyhow!("{e}"))??;
        }
        self.changed();
        Ok(ms)
    }

    /// POSTs the player list to every webhook after each change (changes close together go out as one).
    fn tell_webhooks(self: Arc<Self>) {
        let mut rx = self.changed.subscribe();
        let me = Arc::downgrade(&self);
        drop(self);
        tokio::spawn(async move {
            let http = reqwest::Client::builder().timeout(WEBHOOK_TIMEOUT).build().expect("a plain client builds");
            loop {
                // Behind (more changes than the channel holds) is a change too; closed is Cratarr stopping.
                if let Err(broadcast::error::RecvError::Closed) = rx.recv().await {
                    return;
                }
                tokio::time::sleep(SETTLE).await;
                while rx.try_recv().is_ok() {}
                let Some(me) = me.upgrade() else { return };
                let (hooks, players) = (me.settings().webhooks, me.list());
                let seq = me.seq.fetch_add(1, Ordering::Relaxed) + 1;
                drop(me);
                if hooks.is_empty() {
                    continue;
                }
                let body = serde_json::json!({ "seq": seq, "players": players });
                for url in hooks {
                    if let Err(e) = http.post(&url).json(&body).send().await.and_then(|r| r.error_for_status()) {
                        tracing::warn!("Play On webhook {url}: {e}");
                    }
                }
            }
        });
    }
}

/// What the Snapcast server asks of Play On (decision 0037).
impl crate::snapcast::Groups for PlayOn {
    fn leader_of(&self, player: &str) -> Option<String> {
        PlayOn::leader_of(self, player)
    }

    fn latency_of(&self, player: &str) -> i32 {
        i32::from(self.delay_of(player))
    }

    fn volume_of(&self, player: &str) -> u8 {
        self.entries().get(player).map_or(100, |e| {
            let v = e.report.volume.min(100);
            if e.report.ducked { (f32::from(v) * DUCKED) as u8 } else { v }
        })
    }
}

/// A ducked player plays at this part of its volume (as the app ducks: PlayOnLink's DUCKED).
const DUCKED: f32 = 0.25;

/// The commands for one connected player; dropping it (the connection closed) marks the player offline.
pub struct Commands {
    rx: mpsc::Receiver<String>,
    play_on: Arc<PlayOn>,
    name: String,
    id: u64,
}

impl tokio_stream::Stream for Commands {
    type Item = String;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<String>> {
        self.rx.poll_recv(cx)
    }
}

impl Drop for Commands {
    fn drop(&mut self) {
        self.play_on.disconnected(&self.name, self.id);
    }
}

/// Whether what comes of the album or playlist after place `index` is out of its own order: more than the one turn
/// back that starting partway in gives (as the page tells it, lib/localPlayer.svelte.ts).
fn shuffled_order(tracks: &[QueueTrack], index: i64) -> bool {
    let from = usize::try_from(index + 1).unwrap_or(0);
    let ns: Vec<u32> = tracks.iter().skip(from).filter(|t| t.lane.is_none()).filter_map(|t| t.n).collect();
    ns.windows(2).filter(|w| w[1] < w[0]).count() > 1
}

/// A track as its source knows it, to tell whether two are the same track.
fn key(t: Option<&QueueTrack>) -> Option<(String, String)> {
    t.map(|t| (t.source.clone(), t.id.clone()))
}

fn valid_name(name: &str) -> Result<(), ToggleError> {
    if name.trim().is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
        return Err(ToggleError::BadSettings("a player's name is 1 to 64 characters, no control characters".into()));
    }
    Ok(())
}

/// Tracks of Cratarr's own library (decision 0032) for `play`: an album in its order, the owner's favourites, or
/// tracks from the whole library at random. `None` when none of those is asked for.
async fn own_tracks(from: &PlayFrom) -> Result<Option<Vec<QueueTrack>>, ToggleError> {
    if from.album.is_none() && !from.favourites && !from.library {
        return Ok(None);
    }
    let Some(served) = crate::serve::instance() else { return Err(ToggleError::Refused("Cratarr's own library is not open.".into())) };
    let from = from.clone();
    let songs = tokio::task::spawn_blocking(move || -> Result<Vec<crate::serve::api::Song>> {
        if let Some(album) = &from.album {
            return Ok(served.library.album(album)?.and_then(|a| a.song).unwrap_or_default());
        }
        if from.favourites {
            let owner = served.users.owner().context("there is no Subsonic account yet: make one in Settings")?;
            let ids = served.userdata.starred(&owner, crate::userdata::Kind::Song)?;
            return served.library.songs_by_id(&ids);
        }
        served.library.random_songs(from.limit.unwrap_or(LIBRARY_PICK), None, (None, None), None)
    })
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))??;
    let named = |s: String| Some(s).filter(|s| !s.is_empty());
    Ok(Some(
        songs
            .into_iter()
            .map(|s| QueueTrack {
                source: crate::source::cratarr::KIND.id.to_string(),
                id: s.id,
                name: s.title,
                artist: named(s.artist),
                album: named(s.album),
                album_id: named(s.album_id),
                duration_secs: Some(s.duration as f64).filter(|d| *d > 0.0),
                ..QueueTrack::default()
            })
            .collect(),
    ))
}

/// In place, in a random order (xorshift seeded from the clock: a shuffle needs no better).
pub(super) fn shuffled<T>(items: &mut [T]) {
    let mut x = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) | 1;
    for i in (1..items.len()).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        items.swap(i, (x % (i as u64 + 1)) as usize);
    }
}

/// Written whole (a temporary file, then renamed), readable by the owner only: webhook addresses can carry secrets.
fn write_private(path: &std::path::Path, body: &str) -> Result<()> {
    use std::io::Write;
    let tmp = path.with_extension("json.tmp");
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp).with_context(|| format!("cannot write {}", tmp.display()))?;
    f.write_all(body.as_bytes())?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path).with_context(|| format!("cannot replace {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_read_as_home_assistant_sends_them_and_reach_the_player_resolved() {
        let c: Command = serde_json::from_str(r#"{"command":"play","playlist":"Chill","shuffle":true}"#).unwrap();
        let chill = PlayFrom { playlist: Some("Chill".into()), ..PlayFrom::default() };
        assert_eq!(c, Command::Play { from: chill, tracks: None, index: 0, position_ms: 0, shuffle: true, paused: false });
        let c: Command = serde_json::from_str(r#"{"command":"play","library":true,"shuffle":true,"limit":50}"#).unwrap();
        let library = PlayFrom { library: true, limit: Some(50), ..PlayFrom::default() };
        assert_eq!(c, Command::Play { from: library, tracks: None, index: 0, position_ms: 0, shuffle: true, paused: false });
        let c: Command = serde_json::from_str(r#"{"command":"play","album":"al-x"}"#).unwrap();
        assert!(matches!(c, Command::Play { from: PlayFrom { album: Some(a), favourites: false, .. }, .. } if a == "al-x"));
        let c: Command = serde_json::from_str(r#"{"command":"seek","percent":50}"#).unwrap();
        assert_eq!(c, Command::Seek { position_ms: None, percent: Some(50.0) });
        assert_eq!(serde_json::from_str::<Command>(r#"{"command":"duck"}"#).unwrap(), Command::Duck);
        // To the player: tracks, never the playlist's name or the shuffle wish.
        let to_player = Command::play(vec![], 2, 5);
        assert_eq!(serde_json::to_string(&to_player).unwrap(), r#"{"command":"play","tracks":[],"index":2,"position_ms":5}"#);
        assert_eq!(serde_json::to_string(&Command::Seek { position_ms: Some(9), percent: None }).unwrap(), r#"{"command":"seek","position_ms":9}"#);
        // The sleep timer: a time, the end of the track, or null to clear it; passed on as it came.
        let c: Command = serde_json::from_str(r#"{"command":"sleep","at_ms":1760000000000}"#).unwrap();
        assert_eq!(c, Command::Sleep { at_ms: Some(1_760_000_000_000), end_of_track: false });
        assert_eq!(serde_json::to_string(&c).unwrap(), r#"{"command":"sleep","at_ms":1760000000000}"#);
        let c: Command = serde_json::from_str(r#"{"command":"sleep","at_ms":null,"end_of_track":true}"#).unwrap();
        assert_eq!(serde_json::to_string(&c).unwrap(), r#"{"command":"sleep","at_ms":null,"end_of_track":true}"#);
        let mut q = Queue::default();
        assert_eq!(q.edit(Command::Sleep { at_ms: None, end_of_track: false }).unwrap(), Command::Sleep { at_ms: None, end_of_track: false });
        assert_eq!(q.version, 0);
    }

    #[test]
    fn a_sleep_timer_shows_while_the_player_keeps_one() {
        let mut e = Entry::new();
        assert_eq!(e.view("p", false).sleep, None);
        e.report.sleep = Some(Sleep { at_ms: Some(5), end_of_track: false });
        assert_eq!(e.view("p", false).sleep, Some(Sleep { at_ms: Some(5), end_of_track: false }));
        // One that fired, reported empty, is no timer.
        e.report.sleep = Some(Sleep::default());
        assert_eq!(e.view("p", false).sleep, None);
    }

    #[test]
    fn a_track_shows_its_own_ids_only() {
        let t = |id: &str| QueueTrack { source: "cratarr".into(), id: id.into(), name: id.into(), ..QueueTrack::default() };
        let mut e = Entry::new();
        e.queue.edit(Command::play(vec![t("tr-1"), t("tr-2")], 0, 0)).unwrap();
        e.report.index = 0;
        let ids = TrackIds { track_id: Some("tr-1".into()), album_id: Some("al-one".into()), album_artist: None };
        e.ids = (key(e.queue.tracks.first()), ids.clone());
        assert_eq!(e.view("p", false).ids, ids);
        // Another queue played: Cratarr's copy changes before the player reports, at the same place.
        e.queue.edit(Command::play(vec![t("tr-9")], 0, 0)).unwrap();
        let v = e.view("p", false);
        assert_eq!(v.track.map(|t| t.id).as_deref(), Some("tr-9"));
        assert_eq!(v.ids, TrackIds::default(), "never the last album's ids beside this track");
        assert_ne!(key(e.queue.tracks.first()), e.ids.0, "a new track to look up when the player reports");
    }

    #[test]
    fn a_shuffle_keeps_every_track() {
        let mut v: Vec<u32> = (0..50).collect();
        shuffled(&mut v);
        let mut s = v.clone();
        s.sort();
        assert_eq!(s, (0..50).collect::<Vec<_>>());
        assert_ne!(v, s, "50 tracks in their own order after a shuffle: one chance in 50!");
    }

    /// Play On over an empty library, with no files kept.
    async fn play_on() -> Arc<PlayOn> {
        static N: AtomicU64 = AtomicU64::new(0);
        let d = std::env::temp_dir().join(format!("cratarr-playon-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::create_dir_all(d.join(crate::playlists::PLAYLIST_DIR)).unwrap();
        let lib = super::super::Library::new(None, Some(d.to_string_lossy().into_owned()), Vec::new(), Default::default(), None).await.unwrap();
        let journal = Arc::new(crate::journal::Journal::open(None).unwrap());
        let state = crate::sync_state::SyncState::open(None).unwrap();
        let sync = Arc::new(super::super::sync::SyncService::new(lib.clone(), state, journal.clone()));
        PlayOn::new(lib.clone(), Arc::new(PlaylistService::new(lib, sync, journal)), None)
    }

    /// The commands a player was told since last asked.
    fn told(c: &mut Commands) -> Vec<serde_json::Value> {
        std::iter::from_fn(|| c.rx.try_recv().ok()).map(|m| serde_json::from_str(&m).unwrap()).collect()
    }

    fn said(c: &mut Commands) -> Vec<String> {
        told(c).iter().map(|m| m["command"].as_str().unwrap().to_string()).collect()
    }

    fn group_of(c: &mut Commands) -> Vec<Option<String>> {
        told(c).iter().filter(|m| m["command"] == "group").map(|m| m["leader"].as_str().map(str::to_string)).collect()
    }

    #[tokio::test]
    async fn a_browser_is_a_player_while_open_and_never_in_a_group() {
        let p = play_on().await;
        let t = |id: &str| QueueTrack { source: "cratarr".into(), id: id.into(), name: id.into(), ..QueueTrack::default() };
        p.serve_sync_play(1704, 1000, None);
        let mut page = p.connect_browser("Chrome on Android").unwrap();
        let _tablet = p.connect("tablet").unwrap();
        assert_eq!(p.get("Chrome on Android").unwrap().kind, "browser");
        // Commands reach it as they reach the app.
        p.command("Chrome on Android", Command::play(vec![t("tr-1"), t("tr-2")], 1, 0)).await.unwrap();
        assert!(said(&mut page).contains(&"play".to_string()));
        p.report("Chrome on Android", Report { index: 1, playing: true, ..Report::default() }).unwrap();
        // No sync play in a browser: it joins no group, and none joins it.
        assert!(matches!(p.command("Chrome on Android", Command::Join { to: "tablet".into() }).await, Err(ToggleError::Refused(_))));
        assert!(matches!(p.command("tablet", Command::Join { to: "Chrome on Android".into() }).await, Err(ToggleError::Refused(_))));
        // The page reloaded: it comes back without a queue and gets Cratarr's back.
        drop(page);
        let mut again = p.connect_browser("Chrome on Android").unwrap();
        said(&mut again);
        p.report("Chrome on Android", Report { index: -1, ..Report::default() }).unwrap();
        let back = told(&mut again);
        let play = back.iter().find(|m| m["command"] == "play").expect("its queue given back");
        assert_eq!((play["tracks"].as_array().unwrap().len(), play["index"].as_u64()), (2, Some(1)));
    }

    #[tokio::test]
    async fn a_group_plays_as_one_and_goes_on_without_its_leader() {
        let p = play_on().await;
        let t = |id: &str| QueueTrack { source: "cratarr".into(), id: id.into(), name: id.into(), ..QueueTrack::default() };
        let (mut tablet, mut phone, mut kitchen) = (p.connect("tablet").unwrap(), p.connect("phone").unwrap(), p.connect("kitchen").unwrap());
        // Each is told its group as it connects: none.
        for c in [&mut tablet, &mut phone, &mut kitchen] {
            assert_eq!(group_of(c), vec![None]);
        }
        assert!(matches!(p.command("phone", Command::Join { to: "tablet".into() }).await, Err(ToggleError::Refused(_))), "not served");
        p.serve_sync_play(1704, 1000, Some("192.168.1.5".into()));
        p.command("tablet", Command::play(vec![t("tr-1"), t("tr-2")], 0, 0)).await.unwrap();
        p.report("tablet", Report { index: 0, playing: true, volume: 80, ..Report::default() }).unwrap();
        told(&mut tablet);

        p.command("phone", Command::Join { to: "tablet".into() }).await.unwrap();
        let to_tablet = told(&mut tablet);
        assert_eq!((to_tablet[0]["leader"].as_str(), to_tablet[0]["host"].as_str()), (Some("tablet"), Some("192.168.1.5")));
        assert_eq!(group_of(&mut phone), vec![Some("tablet".to_string())]);
        // The phone shows the group's music, with its own name and volume.
        p.report("phone", Report { index: -1, volume: 30, ..Report::default() }).unwrap();
        let v = p.get("phone").unwrap();
        assert_eq!((v.name.as_str(), v.title.as_deref(), v.volume, v.group.as_deref()), ("phone", Some("tr-1"), 30, Some("tablet")));
        // A command to the phone is the group's; its volume its own.
        p.command("phone", Command::Next).await.unwrap();
        p.command("phone", Command::Volume { level: 40 }).await.unwrap();
        assert_eq!(said(&mut tablet), vec!["next"]);
        assert_eq!(said(&mut phone), vec!["volume"]);

        // Joining the phone is joining the tablet's group; the tablet already leads it.
        p.command("kitchen", Command::Join { to: "phone".into() }).await.unwrap();
        assert_eq!(group_of(&mut kitchen), vec![Some("tablet".to_string())]);
        assert!(told(&mut tablet).is_empty());
        // A Kodi in a group shows as one, not as its leader's kind.
        p.entries().get_mut("kitchen").unwrap().kind = PlayerKind::Kodi;
        assert_eq!(p.get("kitchen").unwrap().kind, "kodi");
        p.entries().get_mut("kitchen").unwrap().kind = PlayerKind::App;

        // The tablet leaves: the phone, joined longest, leads with the tablet's music; the tablet stops.
        p.command("tablet", Command::Leave).await.unwrap();
        let to_tablet = said(&mut tablet);
        assert_eq!((to_tablet.first().map(String::as_str), to_tablet.last().map(String::as_str)), (Some("group"), Some("stop")));
        let to_phone = told(&mut phone);
        assert_eq!(to_phone[0]["leader"], "phone");
        assert_eq!(to_phone[1]["command"], "play");
        assert_eq!(to_phone[1]["tracks"].as_array().unwrap().len(), 2);
        assert_eq!(group_of(&mut kitchen), vec![Some("phone".to_string())]);

        // A speaker delay: kept, shown, at most 500 ms.
        assert_eq!(p.set_delay("kitchen", 120).await.unwrap(), 120);
        assert_eq!(p.get("kitchen").unwrap().delay_ms, 120);
        assert_eq!(p.set_delay("phone", 900).await.unwrap(), MAX_DELAY_MS);
        assert_eq!(crate::snapcast::Groups::latency_of(p.as_ref(), "phone"), 500);
        p.set_delay("phone", 0).await.unwrap();
        assert!(p.settings().delays.get("phone").is_none());

        // The kitchen leaves: the phone, alone, plays on its own.
        p.command("kitchen", Command::Leave).await.unwrap();
        assert_eq!(group_of(&mut kitchen), vec![None]);
        assert_eq!(group_of(&mut phone), vec![None]);
        assert_eq!(p.get("phone").unwrap().group, None);

        // Played on one player, a group ends: the music goes there and the others play alone.
        p.command("kitchen", Command::Join { to: "phone".into() }).await.unwrap();
        p.command("tablet", Command::Join { to: "phone".into() }).await.unwrap();
        told(&mut kitchen);
        told(&mut tablet);
        told(&mut phone);
        p.command("tablet", Command::Handover { to: "kitchen".into() }).await.unwrap();
        assert!(p.list().iter().all(|v| v.group.is_none()));
        assert_eq!(said(&mut kitchen).iter().filter(|c| *c == "play").count(), 1);
        assert_eq!(said(&mut phone).last().map(String::as_str), Some("stop"));
    }

    #[test]
    fn names_that_cannot_be_a_player() {
        assert!(valid_name("Salon tablet").is_ok());
        assert!(valid_name(" ").is_err());
        assert!(valid_name("a\nb").is_err());
        assert!(valid_name(&"x".repeat(65)).is_err());
    }
}
