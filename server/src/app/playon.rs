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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};

use super::queue::{PlayedOn, Queue, Repeat};
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
    /// Its own view of its link to Cratarr (decision 0045); apps from before it leave it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<ReportedLink>,
    /// Why it does not play what it was told to: the device would not let it (decision 0031, amended 2026-10-19). Left
    /// out or null: nothing holds it back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held: Option<String>,
    /// `device` while the app plays the device's own music (decision 0052, local mode): the rest of this report is that
    /// player's, and Cratarr's queue is left as it was. Left out: Cratarr's music, as ever.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// In local mode, the track that plays (the device's own: Cratarr cannot serve it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_track: Option<DeviceTrack>,
    /// Where its sound goes now (decision 0033, amended 2026-10-19), as its app tells: left out by pages, Kodi and
    /// apps from before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Output>,
    /// Which queue it plays, and its name (decision 0033, amended): another id than Cratarr's is a queue the player
    /// started itself, and the one before is kept. Apps from before leave them out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_name: Option<String>,
}

/// A track of a device's own music, as its app tells it in local mode (decision 0052).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceTrack {
    pub name: String,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub album: Option<String>,
}

/// A report's mode for the device's own music (decision 0052).
const DEVICE_MODE: &str = "device";

/// Why Cratarr refuses to change a device's queue in local mode (decision 0052).
const OWN_MUSIC: &str = "That device plays its own music.";

/// A device's audio output (decision 0033, amended 2026-10-19): `bluetooth`, `speaker`, `wired`, `usb`, `hdmi`,
/// `remote` or `other`, and the device's own name where it has one (a car's Bluetooth, say; none for a speaker).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Output {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub name: Option<String>,
}

impl Output {
    /// A queue's name from where it plays: a Bluetooth device's own (a car's), else none.
    fn queue_name(&self) -> Option<String> {
        (self.kind == "bluetooth").then(|| self.name.clone()).flatten().filter(|n| !n.trim().is_empty())
    }
}

/// A player's link as the player sees it (decision 0045): what Cratarr cannot see while the player's reports still get
/// through and its command stream does not. Times are epoch ms on Cratarr's clock.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReportedLink {
    /// Its command stream open since; none while it is not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connected_since: Option<u64>,
    /// Attempts to connect that failed since it was last connected.
    #[serde(default)]
    pub failed: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// The address of Cratarr's it uses (home or public).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// A wait it is in before trying again: until when, and why (a refusal's ten minutes, say).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_until: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_why: Option<String>,
}

/// A player's link as Cratarr knows it (decision 0045), in its view.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct LinkView {
    /// Offline since (epoch seconds); none while online, or not known (offline since Cratarr started).
    pub offline_since: Option<u64>,
    /// How its last connection ended (`replaced`, `gone quiet`, `closed`, `server stopping`), and when (epoch seconds).
    pub ended: Option<&'static str>,
    pub ended_at: Option<u64>,
    /// Its own view, from its last report.
    pub own: Option<ReportedLink>,
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
    /// The play's name, as the one who asks gives it (an album's title, say; decision 0033 amended): else Cratarr's own.
    #[serde(default, skip_serializing)]
    pub name: Option<String>,
    /// A queue taken up whole (one switched back to): nothing of the queue it leaves comes along. Cratarr's own.
    #[serde(skip)]
    pub whole: bool,
    /// Start radio (decision 0036, amended; as the page's "Start radio"): Autoplay seeded from the playlist, album,
    /// favourites or tracks given, else from what plays now, and its first tracks played, as a queue of their own.
    #[serde(default, skip_serializing)]
    pub radio: bool,
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
        /// Which queue this is, and its name (decision 0033, amended: queue history): a player reports them back as it
        /// plays it. Given by the one who asks only to take up a queue again (a handover, a switch).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        queue_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        queue_name: Option<String>,
    },
    /// Tracks into the queue after entry `after` ("" for first), else at place `at`, else at the end. A player is told
    /// both (decision 0033). With `how` (`next`, `last`) they are added by hand, placed as decision 0033's lanes say.
    Add {
        /// What to add when not given tracks: a playlist, an album, the favourites (resolved here, as for `play`).
        #[serde(flatten)]
        from: PlayFrom,
        #[serde(default)]
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
    /// To `position_ms`, or `percent` (0 to 100) of the track, or `by_ms` on from where it is now (back below 0: the
    /// visuals' double and triple taps). What reaches the player is always `position_ms`.
    Seek {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        position_ms: Option<u64>,
        #[serde(default, skip_serializing)]
        percent: Option<f64>,
        #[serde(default, skip_serializing)]
        by_ms: Option<i64>,
    },
    /// 0 to 100.
    Volume { level: u8 },
    /// Quieter, for an announcement; `Unduck` brings it back.
    Duck,
    Unduck,
    /// The queue and position move to player `to`, and this one stops. Handled here; never sent to a player.
    Handover { to: String },
    /// Queue history (decision 0033, amended): the player takes up its queue `id` where it was left, playing if it
    /// played, else paused there; the one it leaves is kept. Handled here; the player is told a play.
    Queue {
        #[serde(default)]
        id: String,
        /// Instead of `id`: the newest queue heard on an output (or player) this names, as "the car queue": this
        /// player's first, then the same account's other players' (decision 0033, amended 2026-10-19). Played.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        played_on: Option<String>,
    },
    /// Queue history: queue `id` (the one that plays too) renamed; kept then until deleted. Handled here.
    QueueRename { id: String, name: String },
    /// Queue history: queue `id` out of the list; not the one that plays. Handled here.
    QueueDelete { id: String },
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
        /// Its own secret, which the sync server asks of it (decision 0051): as a listener, its snapclient's id is
        /// `secret:name`; as a leader, its source hello carries `secret`.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        secret: String,
    },
}

impl Command {
    /// Play these tracks, as a player is told to.
    pub fn play(tracks: Vec<QueueTrack>, index: usize, position_ms: u64) -> Self {
        Command::Play { from: PlayFrom::default(), tracks: Some(tracks), index, position_ms, shuffle: false, paused: false, queue_id: None, queue_name: None }
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
    /// Its link, as Cratarr knows it and as the player last told (decision 0045).
    pub link: LinkView,
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
    /// Why it does not play what it was told to, as it says (its device would not let it); none: nothing holds it.
    pub held: Option<String>,
    /// Where its sound goes now, as its app says (decision 0033, amended 2026-10-19); none where it does not say.
    pub output: Option<Output>,
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
    /// Its queues (decision 0033, amended: queue history), the one that plays marked, newest first: in every view, so
    /// what is pushed (the players stream, Home Assistant's webhook) carries them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queues: Option<Vec<QueueItem>>,
    /// Goes up whenever that list changes: ask for the player by name to see it.
    pub queues_version: u64,
    /// `cratarr`, or `device` while it plays the device's own music (decision 0052): then `title`, `artist`, `album`,
    /// `state` and `position_ms` are that music's, and nothing of Cratarr's queue plays (`track` none, `index` -1,
    /// `queue_length` 0; the queue itself waits, as do its queues).
    pub mode: &'static str,
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
    /// The id the player gave its connection (`X-Cratarr-Connection`; a page's own id), for the log (decision 0045).
    conn: Option<String>,
    /// Offline since, and how its last connection ended, and when (decision 0045).
    offline_since: Option<SystemTime>,
    ended: Option<(&'static str, SystemTime)>,
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
    /// Its queue was given back on this connection, and it has not reported since with the version that came with it.
    giving_back: bool,
    /// The entries of a queue handed over from it while it was offline (the handover's stop could not reach it): back
    /// and still playing one of them, it is stopped then.
    moved: Vec<String>,
    /// The leader of the group it plays in (itself when it leads), decision 0037.
    leader: Option<String>,
    /// When it joined its group, counted: the longest-joined leads when the leader leaves.
    joined: u64,
    kind: PlayerKind,
    /// A page's own id (one per browser tab), while a page is this player: another page may not take its name.
    page: Option<String>,
    /// The account it is a device of (decision 0051): only they play on it. None: from before accounts, the owner's.
    owner: Option<String>,
    /// What it shows the sync server it is (decision 0051), told it with its group; new with each start of Cratarr.
    secret: String,
    /// Its queues left behind, newest first (decision 0033, amended: queue history); a browser keeps its own.
    kept: Vec<Kept>,
    /// While it plays the device's own music (decision 0052, local mode): its last report then, and when it came.
    device: Option<(Report, Instant)>,
    /// Goes up whenever that list changes, so a page asks for it again.
    kept_version: u64,
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
            conn: None,
            offline_since: None,
            ended: None,
            seen: SystemTime::now(),
            heard: Instant::now(),
            ids: (None, TrackIds::default()),
            album_ids: HashMap::new(),
            report: Report { index: -1, volume: 100, ..Report::default() },
            queue: Queue::default(),
            at: Instant::now(),
            fresh: false,
            told: 0,
            giving_back: false,
            moved: Vec::new(),
            leader: None,
            joined: 0,
            kind: PlayerKind::App,
            page: None,
            owner: None,
            secret: new_secret(),
            kept: Vec::new(),
            kept_version: 0,
            device: None,
        }
    }

    /// The queue that plays, as it stands now, to be kept as it is left behind; none when there is none, nor for a
    /// browser (it keeps its own, decision 0033 amended).
    fn standing(&self) -> Option<Kept> {
        if self.queue.tracks.is_empty() || self.kind == PlayerKind::Browser {
            return None;
        }
        let index = usize::try_from(self.index()).ok();
        let entry = index.and_then(|i| self.queue.tracks.get(i)).map(|t| t.entry.clone()).filter(|e| !e.is_empty());
        Some(Kept { queue: self.queue.clone(), entry, position_ms: self.position_now(), playing: self.report.playing, updated: now_secs() })
    }

    /// A queue left behind, kept: first, in place of an earlier copy of it; a queue from before gets an id and a name.
    /// At most `QUEUES` in all with the one that plays: the oldest not renamed goes first, a renamed one stays.
    fn keep(&mut self, k: Option<Kept>) {
        let Some(mut k) = k else { return };
        if k.queue.id.is_empty() {
            k.queue.id = super::queue::new_entry();
        }
        if k.queue.name.is_empty() {
            k.queue.name = name_of(&k.queue.tracks);
        }
        self.kept.retain(|x| x.queue.id != k.queue.id);
        self.kept.insert(0, k);
        while self.kept.len() >= QUEUES {
            let Some(oldest) = self.kept.iter().rposition(|x| !x.queue.renamed) else { break };
            self.kept.remove(oldest);
        }
        self.kept_version += 1;
    }

    /// The list a page shows: the queue that plays (marked) and those left behind, newest first.
    fn queues(&self) -> Vec<QueueItem> {
        let playing = (!self.queue.tracks.is_empty()).then(|| QueueItem {
            id: self.queue.id.clone(),
            name: if self.queue.name.is_empty() { name_of(&self.queue.tracks) } else { self.queue.name.clone() },
            tracks: self.queue.tracks.len(),
            from: self.queue.from.clone(),
            updated: self.queue.started,
            playing: true,
            played_on: self.queue.played_on.clone(),
        });
        let kept = self.kept.iter().map(|k| QueueItem {
            id: k.queue.id.clone(),
            name: k.queue.name.clone(),
            tracks: k.queue.tracks.len(),
            from: k.queue.from.clone(),
            updated: k.updated,
            playing: false,
            played_on: k.queue.played_on.clone(),
        });
        playing.into_iter().chain(kept).collect()
    }

    /// The place of the entry that plays: found by its entry where the player says it, else as the player counts.
    fn index(&self) -> i64 {
        let found = self.report.entry.as_deref().and_then(|e| self.queue.at(e));
        found.map_or(self.report.index, |i| i as i64)
    }

    fn online(&self) -> bool {
        self.connection.is_some() && self.heard.elapsed() < LEASE
    }

    /// Where it is in its track now: as it last reported, moved on by the time since while it plays.
    fn position_now(&self) -> u64 {
        if let Some((d, at)) = &self.device {
            let mut p = d.position_ms;
            if d.playing {
                p += at.elapsed().as_millis() as u64;
                if let Some(len) = d.duration_ms {
                    p = p.min(len);
                }
            }
            return p;
        }
        let r = &self.report;
        let mut position_ms = r.position_ms;
        if r.playing {
            position_ms += self.at.elapsed().as_millis() as u64;
            if let Some(d) = r.duration_ms {
                position_ms = position_ms.min(d);
            }
        }
        position_ms
    }

    fn view(&self, name: &str, with_queue: bool) -> PlayerView {
        let mut v = self.cratarr_view(name, with_queue);
        // Local mode (decision 0052): the device's own music is what plays.
        if let Some((d, at)) = &self.device {
            let t = d.device_track.clone().unwrap_or_default();
            let mut position_ms = d.position_ms;
            if d.playing {
                position_ms += at.elapsed().as_millis() as u64;
                if let Some(len) = d.duration_ms {
                    position_ms = position_ms.min(len);
                }
            }
            v.mode = DEVICE_MODE;
            v.state = if d.playing { "playing" } else { "paused" };
            (v.playing, v.position_ms, v.duration_ms, v.index, v.queue_length) = (d.playing, position_ms, d.duration_ms, -1, 0);
            (v.title, v.artist, v.album) = (Some(t.name).filter(|n| !n.is_empty()), t.artist, t.album);
            (v.track, v.ids, v.current_entry_id) = (None, TrackIds::default(), None);
            v.queue = with_queue.then(Vec::new);
        }
        v
    }

    /// The view of Cratarr's music on it.
    fn cratarr_view(&self, name: &str, with_queue: bool) -> PlayerView {
        let r = &self.report;
        let position_ms = self.position_now();
        let index = self.index();
        let track = usize::try_from(index).ok().and_then(|i| self.queue.tracks.get(i)).cloned().map(|mut t| {
            if t.album_id.is_none() {
                t.album_id = self.album_ids.get(&format!("{}\n{}", t.source, t.id)).cloned();
            }
            t
        });
        let secs = |t: SystemTime| t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        PlayerView {
            name: name.to_string(),
            online: self.online(),
            link: LinkView {
                offline_since: self.offline_since.filter(|_| !self.online()).map(secs),
                ended: self.ended.map(|(why, _)| why),
                ended_at: self.ended.map(|(_, at)| secs(at)),
                own: self.report.link.clone(),
            },
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
            held: r.held.clone(),
            output: r.output.clone(),
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
            queues: Some(self.queues()),
            queues_version: self.kept_version,
            mode: "cratarr",
        }
    }
}

/// Whose players the running request reaches (decision 0051): its account's own. `None`: every one, for Cratarr's own
/// work (a task of its own, a signed link), and for the owner while signing in is off (one user, whose they all are).
pub fn whose() -> Option<String> {
    let served = crate::serve::instance()?;
    let user = crate::serve::who::current()?.user;
    if !served.sign_in_required() && served.users.owner().as_deref() == Some(user.as_str()) {
        return None;
    }
    Some(user)
}

/// The first administrator: a player from before accounts is theirs.
fn first_owner() -> Option<String> {
    crate::serve::instance().and_then(|s| s.users.owner())
}

/// Whether `e` is `user`'s (`first`: the first administrator's name).
fn belongs_to(e: &Entry, user: &str, first: Option<&str>) -> bool {
    e.owner.as_deref().or(first) == Some(user)
}

/// The player `name` for whose it is to be: made theirs when new; another account's is refused (a name is one
/// player's, and one plays on one's own devices only).
fn claim<'a>(entries: &'a mut HashMap<String, Entry>, name: &str, whose: Option<&str>) -> Result<&'a mut Entry, ToggleError> {
    if let (Some(user), Some(e)) = (whose, entries.get(name))
        && !belongs_to(e, user, first_owner().as_deref())
    {
        return Err(ToggleError::NameTaken(name.to_string()));
    }
    let e = entries.entry(name.to_string()).or_insert_with(Entry::new);
    if e.owner.is_none() {
        e.owner = whose.map(str::to_string);
    }
    Ok(e)
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
            link: own.link,
            seen: own.seen,
            volume: own.volume,
            ducked: own.ducked,
            held: own.held,
            output: own.output,
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
    /// Cratarr is stopping: the connections that end now end for that.
    stopping: AtomicBool,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owner: Option<String>,
    /// Its queues left behind (decision 0033, amended).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    kept: Vec<Kept>,
}

/// A queue left behind (decision 0033, amended: queue history), whole: its tracks with their entries, its repeat,
/// Radio and Autoplay, and where it stood (the entry that played, the position, whether it played).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct Kept {
    #[serde(flatten)]
    queue: Queue,
    #[serde(default)]
    entry: Option<String>,
    #[serde(default)]
    position_ms: u64,
    #[serde(default)]
    playing: bool,
    /// When it was left (epoch seconds).
    #[serde(default)]
    updated: u64,
}

/// A queue in a player's list, as a page shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QueueItem {
    pub id: String,
    pub name: String,
    /// How many tracks.
    pub tracks: usize,
    /// What it was started from (`playlist`, `album`, `favourites`, `library`, `tracks`, `player`).
    pub from: Option<String>,
    /// When it was left (epoch seconds); for the one that plays, when it started (0: not known).
    pub updated: u64,
    pub playing: bool,
    /// Where it was heard, oldest first.
    pub played_on: Vec<PlayedOn>,
}

/// Queues per player, with the one that plays (decision 0033, amended).
const QUEUES: usize = 20;
/// A queue's name, at most this many characters.
const QUEUE_NAME_MAX: usize = 100;
/// How many tracks a radio started here plays first (as the page's Autoplay asks at a time).
const RADIO_START: usize = 5;

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A queue's name when none was given: its album's, else its artist's, else its first track's.
fn name_of(tracks: &[QueueTrack]) -> String {
    let own: Vec<&QueueTrack> = tracks.iter().filter(|t| t.lane.is_none()).collect();
    let own = if own.is_empty() { tracks.iter().collect() } else { own };
    let same = |f: fn(&QueueTrack) -> Option<&String>| {
        let first = own.first().and_then(|t| f(t))?;
        own.iter().all(|t| f(t) == Some(first)).then(|| first.clone())
    };
    same(|t| t.album.as_ref()).or_else(|| same(|t| t.artist.as_ref())).or_else(|| own.first().map(|t| t.name.clone())).unwrap_or_default()
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
                if e.queue.id.is_empty() && !e.queue.tracks.is_empty() {
                    (e.queue.id, e.queue.name) = (super::queue::new_entry(), name_of(&e.queue.tracks));
                }
                e.owner = s.owner;
                e.kept = s.kept;
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
            stopping: AtomicBool::new(false),
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

    /// A Kodi's driver joins as a player (`app::kodi`), a device of the administrator who added it.
    pub(super) fn connect_kodi(self: &Arc<Self>, name: &str, owner: Option<&str>) -> Result<Commands, ToggleError> {
        let commands = self.connect_as(name, PlayerKind::Kodi, None, None)?;
        if let Some(e) = self.entries().get_mut(name) {
            e.owner = owner.map(str::to_string);
        }
        Ok(commands)
    }

    /// A page in a browser joins as a player while it is open (decision 0033; 0031 amended).
    /// `page`: the page's own id (one per tab). A name held now by another page, or by the app or a Kodi, is refused:
    /// two players under one name pushed each other off every few seconds (2026-10-16, two tabs both "Chrome on
    /// Android"). The same page coming back (a reload, a new connection) takes its name back.
    pub fn connect_browser(self: &Arc<Self>, name: &str, page: Option<&str>) -> Result<Commands, ToggleError> {
        if let Some(e) = self.entries().get(name)
            && e.online()
            && (e.kind != PlayerKind::Browser || e.page.as_deref() != page)
        {
            return Err(ToggleError::Refused(format!("{name} is another player's name.")));
        }
        self.connect_as(name, PlayerKind::Browser, page, page)
    }

    /// The app joins as a player (a name a page used before is the app's again); `conn`: the id it gave this
    /// connection (decision 0045).
    pub fn connect_app(self: &Arc<Self>, name: &str, conn: Option<&str>) -> Result<Commands, ToggleError> {
        self.connect_as(name, PlayerKind::App, None, conn)
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
        let online: Vec<String> = self.list().into_iter().filter(|v| v.online).map(|v| v.name).collect();
        let asked: Vec<String> = online.into_iter().filter(|n| self.send(n, &Command::Sample { id }).is_ok()).collect();
        self.samples.wait(id, &asked).await
    }

    /// A player's answer to a measurement.
    pub fn sample(&self, name: &str, s: super::together::Sample) -> Result<(), ToggleError> {
        valid_name(name)?;
        self.reached(name)?;
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
        let (whose, first) = (whose(), first_owner());
        let entries = self.entries();
        let online = entries.iter().filter(|(_, e)| e.online() && whose.as_deref().is_none_or(|u| belongs_to(e, u, first.as_deref())));
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
                // Its connection still open but nothing heard for the lease: it ended, gone quiet, when last heard from.
                let quiet: Vec<(String, Option<String>)> = me
                    .entries()
                    .iter_mut()
                    .filter(|(n, e)| was.get(*n) == Some(&true) && now.get(*n) == Some(&false) && e.connection.is_some())
                    .map(|(n, e)| {
                        e.ended = Some(("gone quiet", SystemTime::now()));
                        e.offline_since = SystemTime::now().checked_sub(e.heard.elapsed());
                        (n.clone(), e.conn.clone())
                    })
                    .collect();
                for (n, conn) in quiet {
                    tracing::info!(target: "playon", player = n.as_str(), conn = conn.as_deref(), reason = "gone quiet", "connection ended");
                }
                was = now;
                // Gone (its lease over) leaves its group, which plays on; back, it does not join again by itself.
                let gone: Vec<String> = me.entries().iter().filter(|(_, e)| e.leader.is_some() && e.heard.elapsed() >= LEASE).map(|(n, _)| n.clone()).collect();
                for n in gone {
                    tracing::info!(target: "playon", player = n.as_str(), reason = "gone quiet", "out of its group");
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
                        let (queue, owner, kept) = (e.queue.clone(), e.owner.clone(), e.kept.clone());
                        (n.clone(), Saved { queue, entry: v.current_entry_id, position_ms: v.position_ms, playing: v.playing, owner, kept })
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
                    Ok(Err(e)) => {
                        let error = format!("{e:#}");
                        tracing::error!(target: "playon", error, "cannot save the queues");
                    }
                    Err(e) => {
                        let error = e.to_string();
                        tracing::error!(target: "playon", error, "cannot save the queues");
                    }
                }
            }
        });
    }

    /// Told on every change of a player (the pages' live channel).
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.changed.subscribe()
    }

    /// Every player of the running request's account (`whose`) Cratarr has heard of since it started, by name.
    pub fn list(&self) -> Vec<PlayerView> {
        self.list_for(whose().as_deref())
    }

    /// `user`'s players (`None`: every one).
    pub fn list_for(&self, user: Option<&str>) -> Vec<PlayerView> {
        let first = first_owner();
        let entries = self.entries();
        let mut out: Vec<PlayerView> = entries
            .iter()
            .filter(|(_, e)| user.is_none_or(|u| belongs_to(e, u, first.as_deref())))
            .filter_map(|(n, _)| view_in(&entries, n, false))
            .collect();
        drop(entries);
        let delays = self.settings().delays;
        for v in &mut out {
            v.delay_ms = delays.get(&v.name).copied().unwrap_or(0);
        }
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        out
    }

    /// Whether the running request reaches player `name` (`whose`): else it is as if there were none.
    pub fn reached(&self, name: &str) -> Result<(), ToggleError> {
        let Some(user) = whose() else { return Ok(()) };
        let first = first_owner();
        match self.entries().get(name) {
            Some(e) if !belongs_to(e, &user, first.as_deref()) => Err(ToggleError::Refused(format!("No player is called {name}."))),
            _ => Ok(()),
        }
    }

    /// Forgets `user`'s player `name` (decision 0051: one's devices, removed): not while it is connected, nor a Kodi
    /// (the Players settings' to remove). `false`: no such player of theirs.
    pub fn forget(&self, name: &str, user: &str) -> Result<bool, ToggleError> {
        let first = first_owner();
        let mut entries = self.entries();
        match entries.get(name) {
            Some(e) if belongs_to(e, user, first.as_deref()) => {
                if e.online() {
                    return Err(ToggleError::Refused(format!("{name} is connected: close Cratarr there first.")));
                }
                if e.kind == PlayerKind::Kodi {
                    return Err(ToggleError::Refused(format!("{name} is a Kodi: remove it in Players.")));
                }
            }
            _ => return Ok(false),
        }
        entries.remove(name);
        drop(entries);
        self.settings.lock().unwrap_or_else(|p| p.into_inner()).delays.remove(name);
        tracing::info!(target: "playon", player = name, user, "forgotten");
        self.changed();
        Ok(true)
    }

    /// One player, with its queue.
    pub fn get(&self, name: &str) -> Option<PlayerView> {
        self.reached(name).ok()?;
        let v = view_in(&self.entries(), name, true);
        v.map(|v| PlayerView { delay_ms: self.delay_of(name), ..v })
    }

    /// An app's player, without a connection id (the tests').
    #[cfg(test)]
    pub fn connect(self: &Arc<Self>, name: &str) -> Result<Commands, ToggleError> {
        self.connect_as(name, PlayerKind::App, None, None)
    }

    /// A player connects: the commands for it come out of the returned stream, which closes the connection when it
    /// is dropped (the player went away). A second connection under the same name replaces the first.
    fn connect_as(self: &Arc<Self>, name: &str, kind: PlayerKind, page: Option<&str>, conn: Option<&str>) -> Result<Commands, ToggleError> {
        valid_name(name)?;
        let (tx, rx) = mpsc::channel(16);
        let id = self.connections.fetch_add(1, Ordering::Relaxed);
        let replaced = {
            let mut entries = self.entries();
            let e = claim(&mut entries, name, whose().as_deref())?;
            let replaced = e.connection.is_some().then(|| e.conn.clone());
            if replaced.is_some() {
                e.ended = Some(("replaced", SystemTime::now()));
            }
            e.connection = Some((id, tx));
            e.conn = conn.map(str::to_string);
            e.kind = kind;
            e.page = page.map(str::to_string);
            e.offline_since = None;
            e.seen = SystemTime::now();
            e.heard = Instant::now();
            e.fresh = true;
            e.giving_back = false;
            replaced
        };
        if let Some(old) = replaced {
            tracing::info!(target: "playon", player = name, conn = old.as_deref(), reason = "replaced", "connection ended");
        }
        tracing::info!(target: "playon", player = name, conn, kind = kind.name(), "connection opened");
        // Its group as Cratarr knows it (none, after it was gone): what it played with before it went is set straight.
        self.tell_group(name);
        self.changed();
        Ok(Commands { rx, play_on: self.clone(), name: name.to_string(), id })
    }

    /// Its connection closed: by the player or the network (Cratarr cannot tell which; the player's own log can).
    fn disconnected(&self, name: &str, id: u64) {
        let mut entries = self.entries();
        let Some(e) = entries.get_mut(name) else { return };
        if e.connection.as_ref().is_some_and(|(c, _)| *c == id) {
            e.connection = None;
            e.offline_since.get_or_insert_with(SystemTime::now);
            let stopping = self.stopping.load(Ordering::Relaxed);
            if !stopping {
                e.ended = Some(("closed", SystemTime::now()));
            }
            let conn = e.conn.clone();
            drop(entries);
            if !stopping {
                tracing::info!(target: "playon", player = name, conn = conn.as_deref(), reason = "closed", "connection ended");
            }
            self.changed();
        }
    }

    /// Cratarr is stopping: each player connected now has its connection end for that (decision 0045).
    pub fn stopping(&self) {
        self.stopping.store(true, Ordering::Relaxed);
        let ended: Vec<(String, Option<String>)> = self
            .entries()
            .iter_mut()
            .filter(|(_, e)| e.connection.is_some())
            .map(|(n, e)| {
                e.ended = Some(("server stopping", SystemTime::now()));
                (n.clone(), e.conn.clone())
            })
            .collect();
        for (n, conn) in ended {
            tracing::info!(target: "playon", player = n.as_str(), conn = conn.as_deref(), reason = "server stopping", "connection ended");
        }
    }

    /// What a player says it plays.
    pub fn report(self: &Arc<Self>, name: &str, report: Report) -> Result<(), ToggleError> {
        valid_name(name)?;
        let mut entries = self.entries();
        let e = claim(&mut entries, name, whose().as_deref())?;
        let before = e.view(name, false);
        // Local mode (decision 0052): what plays there is kept as said, its volume, output and link with it; Cratarr's
        // queue, the queue given back on a return, and its history are left alone until the app plays Cratarr again.
        if report.mode.as_deref() == Some(DEVICE_MODE) {
            e.seen = SystemTime::now();
            e.heard = Instant::now();
            (e.report.volume, e.report.ducked) = (report.volume, report.ducked);
            (e.report.output, e.report.link, e.report.held) = (report.output.clone(), report.link.clone(), report.held.clone());
            e.device = Some((report, Instant::now()));
            let after = e.view(name, false);
            drop(entries);
            let still = |v: &PlayerView| PlayerView { position_ms: 0, position_at: 0, seen: 0, ..v.clone() };
            if still(&before) != still(&after) {
                self.changed();
            }
            return Ok(());
        }
        e.device = None;
        e.seen = SystemTime::now();
        e.heard = Instant::now();
        // Back without a queue (the app was updated, or killed): it gets its queue back where it was, playing on if it
        // played (the owner's wish, 2026-10-15), else paused.
        let came_back_empty = std::mem::take(&mut e.fresh) && report.index < 0 && !e.queue.tracks.is_empty() && e.kind != PlayerKind::Kodi;
        let given = came_back_empty.then(|| (e.queue.tracks.len(), before.playing));
        let give_back = came_back_empty.then(|| {
            // A version of its own: what the player was told before (its group, as it connected, carries the queue's
            // version too) must not count as having this queue (the tablet's queue was taken as empty at 00:36,
            // 2026-10-18, by a report with its group's version).
            e.queue.version += 1;
            let (index, position_ms, paused) = (before.index.max(0) as usize, before.position_ms, !before.playing);
            let (queue_id, queue_name) = ((!e.queue.id.is_empty()).then(|| e.queue.id.clone()), (!e.queue.name.is_empty()).then(|| e.queue.name.clone()));
            Command::Play { from: PlayFrom::default(), tracks: Some(e.queue.tracks.clone()), index, position_ms, shuffle: false, paused, queue_id, queue_name }
        });
        // Back after its music was handed away (its stop could not reach it): still playing that music, it stops now, as
        // it would have then; what was started on it since is its own.
        let stop_moved = e.connection.is_some() && !e.moved.is_empty() && {
            let moved = std::mem::take(&mut e.moved);
            report.entry.as_deref().map_or(report.index >= 0, |x| moved.iter().any(|m| m == x))
        };
        let mut adopted = None;
        // Given its queue back, a player reports from before that queue reaches it (without a version, or with an
        // older one): it says what it held before, nothing, and neither empties Cratarr's queue nor stands for what it
        // plays (the tablet's queue was taken as empty for 0.75 s after its app was updated, 2026-10-17). Its first
        // report with the given version counts again.
        let before_given = e.giving_back && report.version.is_none_or(|v| v < e.told);
        if report.version.is_some_and(|v| v >= e.told) {
            e.giving_back = false;
        }
        // Back without a queue before its command stream is open (an app reports as it starts, and may before it
        // connects): what it says is not taken, and its queue is given back once it is connected (the tablet's was taken
        // as empty at 13:23, 2026-10-19, after its app was updated, and was lost).
        let early_empty = e.connection.is_none() && report.index < 0 && !e.queue.tracks.is_empty() && e.kind != PlayerKind::Kodi;
        if give_back.is_some() {
            e.giving_back = true;
            // Told here already (`send` tells it again): a report between this lock and the send must not count.
            e.told = e.queue.version;
            e.at = Instant::now();
        } else if !before_given && !early_empty {
            // Taken as the player holds it, unless it was made before Cratarr's latest edit reached the player: that edit
            // is on its way, and the player reports again once it has it (no flip back and forth).
            if report.version.is_none_or(|v| v >= e.told) {
                // Another queue (decision 0033, amended): one the player names itself, something new it started, or
                // none at all. The one before is kept as it stood.
                let standing = e.standing();
                let named = report.queue_id.clone().filter(|id| *id != e.queue.id);
                let other = match report.queue.clone().map(|q| e.queue.adopted(q)) {
                    Some(Some(other)) => {
                        adopted = Some((e.queue.tracks.len(), e.queue.version));
                        other
                    }
                    _ => false,
                };
                if named.is_some() || other {
                    e.keep(standing);
                    // Without a name of its own: its Bluetooth device's (a car's), else after its tracks.
                    let given = report.queue_name.clone().filter(|n| !n.trim().is_empty());
                    let unnamed = || report.output.as_ref().and_then(Output::queue_name).unwrap_or_else(|| name_of(&e.queue.tracks));
                    let (id, name) = match named {
                        Some(id) => (id, given.unwrap_or_else(unnamed)),
                        None if e.queue.tracks.is_empty() => (String::new(), String::new()),
                        None => (super::queue::new_entry(), unnamed()),
                    };
                    e.queue.from = (!id.is_empty()).then(|| "player".to_string());
                    (e.queue.id, e.queue.name, e.queue.renamed) = (id, name, false);
                    e.queue.played_on.clear();
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
            // Where its queue is heard (decision 0033, amended 2026-10-19): this player, through its output.
            if e.report.playing && !e.queue.tracks.is_empty() && e.kind != PlayerKind::Browser {
                let out = e.report.output.clone();
                e.queue.heard(PlayedOn { player: name.to_string(), kind: out.as_ref().map(|o| o.kind.clone()), name: out.and_then(|o| o.name) });
            }
        }
        let track = e.view(name, false).track;
        let new_track = key(track.as_ref()) != e.ids.0;
        drop(entries);
        if let (Some(c), Some((tracks, playing))) = (give_back, given) {
            tracing::info!(target: "playon", player = name, tracks, playing, "queue given back");
            // Not sent, nothing is on its way: its reports count as they come.
            if self.send(name, &c).is_err()
                && let Some(e) = self.entries().get_mut(name)
            {
                e.giving_back = false;
            }
        }
        if let Some((tracks, version)) = adopted {
            tracing::info!(target: "playon", player = name, tracks, version, "queue adopted");
        }
        if stop_moved {
            tracing::info!(target: "playon", player = name, "stopped on its return");
            let _ = self.send(name, &Command::Stop);
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
        // One's own players only, and only with one's own (decision 0051).
        self.reached(name)?;
        if let Command::Join { to } | Command::Handover { to } = &command {
            self.reached(to)?;
        }
        // A command to any player of a group is the group's, played by its leader; its volume stays its own.
        let own = matches!(command, Command::Volume { .. } | Command::Duck | Command::Unduck | Command::Sample { .. } | Command::Join { .. } | Command::Leave);
        let leader = if own { None } else { self.leader_of(name) };
        let name = leader.as_deref().unwrap_or(name);
        let mut started: Option<&'static str> = None;
        // Local mode (decision 0052): steered, not edited. A play (or a queue taken up, or a handover to it) is Cratarr's
        // music again, and the app goes back to it; a stop pauses it, so Cratarr's waiting queue stays.
        let command = if self.entries().get(name).is_some_and(|e| e.device.is_some()) {
            match command {
                Command::Pause | Command::Resume | Command::Toggle | Command::Next | Command::Previous | Command::Volume { .. } => command,
                Command::Duck | Command::Unduck | Command::Sleep { .. } | Command::Seek { .. } | Command::Sample { .. } => command,
                Command::Play { .. } | Command::Queue { .. } | Command::QueueRename { .. } | Command::QueueDelete { .. } => command,
                Command::Stop => Command::Pause,
                _ => return Err(ToggleError::Refused(OWN_MUSIC.into())),
            }
        } else {
            command
        };
        let command = match command {
            Command::Join { to } => return self.join(name, &to).await,
            Command::Leave => return self.leave(name).await,
            Command::Group { .. } => return Err(ToggleError::Refused("A group is made with join and leave.".into())),
            // Played on one player: the group ends (decision 0037).
            Command::Handover { to } => {
                self.dissolve(name);
                return self.handover(name, &to).await;
            }
            Command::Queue { played_on: Some(text), .. } => return self.switch_to_played_on(name, &text).await,
            Command::Queue { id, .. } => return self.switch(name, &id).await,
            Command::QueueRename { id, name: new } => return self.rename_queue(name, &id, &new),
            Command::QueueDelete { id } => return self.delete_queue(name, &id),
            Command::Play { from, tracks, .. } if from.radio => return self.start_radio(name, from, tracks).await,
            Command::Play { from, tracks, mut index, mut position_ms, shuffle, paused, queue_id, queue_name } => {
                // A new queue (decision 0033, amended): what it was started from, kept with it; one taken up again (a
                // handover, a switch) comes with its own id, and keeps what it had.
                if queue_id.is_none() {
                    started = Some(match () {
                        _ if from.playlist.is_some() => "playlist",
                        _ if from.album.is_some() => "album",
                        _ if from.favourites => "favourites",
                        _ if from.library => "library",
                        _ => "tracks",
                    });
                }
                let picked = self.picked(name, &from).await?;
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
                let tracks = self.named(tracks).await;
                // Its name: as given, else the playlist's, else Cratarr's own (the album's title, say).
                // Not a playlist or an album, and the player's sound goes to a Bluetooth device of its own name (a car's):
                // named after that device (decision 0033, amended 2026-10-19).
                let device = (from.album.is_none() && queue_name.is_none() && from.name.is_none())
                    .then(|| self.entries().get(name).and_then(|e| e.report.output.as_ref().and_then(Output::queue_name)))
                    .flatten();
                let queue_name = queue_name.or(from.name).or(from.playlist).or(device).unwrap_or_else(|| match started {
                    Some("favourites") => "Favourites".into(),
                    Some("library") => "Library".into(),
                    _ => name_of(&tracks),
                });
                let queue_id = Some(queue_id.unwrap_or_else(super::queue::new_entry));
                let from = PlayFrom { whole: from.whole, ..PlayFrom::default() };
                Command::Play { from, tracks: Some(tracks), index, position_ms, shuffle, paused, queue_id, queue_name: Some(queue_name) }
            }
            // By name too (a playlist, an album, the favourites), as `play` takes them: Home Assistant's "add this album".
            Command::Add { from, tracks, at, after, how } => {
                let picked = self.picked(name, &from).await?;
                let tracks = picked.unwrap_or(tracks);
                if tracks.is_empty() {
                    return Err(ToggleError::Refused("Add what? Give tracks, a playlist, an album or the favourites.".into()));
                }
                Command::Add { from: PlayFrom::default(), tracks: self.named(tracks).await, at, after, how }
            }
            Command::Seek { position_ms: None, percent: Some(p), .. } => {
                let duration = self.entries().get(name).and_then(|e| e.report.duration_ms);
                let Some(d) = duration else { return Err(ToggleError::Refused("The track's length is not known yet.".into())) };
                Command::Seek { position_ms: Some((d as f64 * p.clamp(0.0, 100.0) / 100.0) as u64), percent: None, by_ms: None }
            }
            // On or back from where it is now (as Cratarr reckons it: its last report moved on), never past the start,
            // and short of the end by a second (a seek past it would skip the track).
            Command::Seek { position_ms: None, percent: None, by_ms: Some(by) } => {
                let (now, duration) = match self.entries().get(name) {
                    Some(e) => (e.position_now(), e.report.duration_ms),
                    None => return Err(ToggleError::Refused(format!("No player is called {name}."))),
                };
                let mut to = (now as i64 + by).max(0) as u64;
                if let Some(d) = duration {
                    to = to.min(d.saturating_sub(1000));
                }
                Command::Seek { position_ms: Some(to), percent: None, by_ms: None }
            }
            Command::Seek { position_ms: None, percent: None, by_ms: None } => return Err(ToggleError::Refused("Seek where?".into())),
            other => other,
        };
        // The queue is edited here first (decision 0033), and the player told the edit by entry.
        let mut entries = self.entries();
        let Some(e) = entries.get_mut(name) else { return Err(ToggleError::Refused(format!("No player is called {name}."))) };
        if !e.online() {
            tracing::warn!(target: "playon", player = name, command = command_name(&command).as_str(), reason = "offline", "command not delivered");
            return Err(ToggleError::Refused(format!("{name} is not connected.")));
        }
        let leaves = matches!(command, Command::Play { .. } | Command::Stop);
        let standing = if leaves { e.standing() } else { None };
        let mut queue = e.queue.clone();
        let told = queue.edit_at(command, usize::try_from(e.index()).ok())?;
        Self::tell(e, name, &told, queue.version)?;
        let edited = queue != e.queue;
        if edited {
            e.told = queue.version;
        }
        if leaves {
            // Kept only once a queue took its place, or it stopped (decision 0033, amended).
            if standing.as_ref().is_some_and(|s| s.queue.id != queue.id || queue.tracks.is_empty()) {
                e.keep(standing);
            }
            if queue.tracks.is_empty() {
                (queue.id, queue.name, queue.renamed, queue.from) = (String::new(), String::new(), false, None);
            } else if let Some(kind) = started {
                queue.from = Some(kind.to_string());
            }
            let before = e.kept.len();
            e.kept.retain(|k| k.queue.id != queue.id);
            if e.kept.len() != before {
                e.kept_version += 1;
            }
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
        let what = json["command"].as_str().unwrap_or_default().to_string();
        let Some((_, tx)) = &e.connection else {
            tracing::warn!(target: "playon", player = name, command = what.as_str(), reason = "offline", "command not delivered");
            return Err(ToggleError::Refused(format!("{name} is not connected.")));
        };
        tx.try_send(json.to_string()).map_err(|err| {
            let reason = if matches!(err, mpsc::error::TrySendError::Full(_)) { "too many waiting" } else { "closed" };
            tracing::warn!(target: "playon", player = name, command = what.as_str(), reason, "command not delivered");
            ToggleError::Refused(format!("{name} does not take commands now."))
        })
    }

    /// The queue and position of `from` go to `to`, which plays them, with their entries; `from` stops (decision 0033).
    /// The queue moves on as itself, its id and name with it, and the player it left keeps it as it stood (0033 amended).
    async fn handover(&self, from: &str, to: &str) -> Result<(), ToggleError> {
        tracing::info!(target: "playon", player = from, to, "handover");
        if self.entries().get(from).is_some_and(|e| e.device.is_some()) {
            return Err(ToggleError::Refused(OWN_MUSIC.into()));
        }
        let (queue, index, position_ms, sleep) = {
            let entries = self.entries();
            let Some(e) = entries.get(from) else { return Err(ToggleError::Refused(format!("No player is called {from}."))) };
            let v = e.view(from, false);
            (e.queue.clone(), v.index.max(0) as usize, v.position_ms, e.report.sleep)
        };
        if queue.tracks.is_empty() {
            return Err(ToggleError::Refused(format!("{from} has nothing to hand over.")));
        }
        let id = if queue.id.is_empty() { super::queue::new_entry() } else { queue.id.clone() };
        let name = if queue.name.is_empty() { name_of(&queue.tracks) } else { queue.name.clone() };
        let play = Command::Play {
            from: PlayFrom::default(),
            tracks: Some(queue.tracks.clone()),
            index,
            position_ms,
            shuffle: false,
            paused: false,
            queue_id: Some(id),
            queue_name: Some(name),
        };
        Box::pin(self.command(to, play)).await?;
        self.settle(to, &queue).await;
        // A sleep timer moves with the music.
        if let Some(s) = sleep.filter(|s| s.at_ms.is_some() || s.end_of_track) {
            let _ = Box::pin(self.command(to, Command::Sleep { at_ms: s.at_ms, end_of_track: s.end_of_track })).await;
        }
        // The music is on its way there; this one stopping is no reason to fail the handover. One that cannot be told
        // (offline) is stopped here as the stop would have left it, and stopped itself once it is back.
        if Box::pin(self.command(from, Command::Stop)).await.is_err() {
            self.stopped_away(from);
        }
        Ok(())
    }

    /// A queue's settings on the player that takes it up (a handover, a switch): repeat (repeat one too, decision 0033
    /// amended), Radio with its choice and count, favourites in or out, and Autoplay with its seed and bound (decision
    /// 0036, amended). Autoplay last (on turns repeat all off, and both came so). Its queue keeps what it was started
    /// from, and a name the owner gave it.
    async fn settle(&self, to: &str, queue: &Queue) {
        let _ = Box::pin(self.command(to, Command::Repeat { mode: queue.repeat })).await;
        let radio = Command::Radio {
            on: queue.radio,
            mode: None,
            seed: None,
            within: None,
            by: Some(queue.radio_by),
            count: Some(queue.radio_count().max(1)),
        };
        let _ = Box::pin(self.command(to, radio)).await;
        let _ = Box::pin(self.command(to, Command::SkipFavourites { on: queue.skip_favourites })).await;
        let auto = match queue.autoplay.clone() {
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
        let target = self.leader_of(to).unwrap_or_else(|| to.to_string());
        if let Some(e) = self.entries().get_mut(&target)
            && e.queue.id == queue.id
        {
            (e.queue.from, e.queue.renamed, e.queue.played_on) = (queue.from.clone(), queue.renamed, queue.played_on.clone());
        }
    }

    /// The tracks a play or an add names: a playlist's, an album's, the favourites, the library; None when it names
    /// none (it brings its tracks).
    async fn picked(&self, name: &str, from: &PlayFrom) -> Result<Option<Vec<QueueTrack>>, ToggleError> {
        match &from.playlist {
            Some(p) => Ok(Some(self.playlist_tracks(p).await?)),
            None => own_tracks(from, self.owner_of(name)).await,
        }
    }

    /// Start radio (decision 0036, amended), as the page's "Start radio" does it: Autoplay on, seeded from what the play
    /// names (a playlist, an album, the favourites, tracks) or else from the track that plays, in the player's own
    /// Autoplay mode (Similar at first); its first tracks play as a queue named "Radio: …".
    async fn start_radio(&self, name: &str, from: PlayFrom, tracks: Option<Vec<QueueTrack>>) -> Result<(), ToggleError> {
        let seeds = match self.picked(name, &from).await?.or(tracks) {
            Some(t) => t,
            None => self.get(name).and_then(|v| v.track).into_iter().collect(),
        };
        let own = crate::source::cratarr::KIND.id;
        let seed: Vec<String> = seeds.iter().filter(|t| t.source == own).map(|t| t.id.clone()).collect();
        if seed.is_empty() {
            return Err(ToggleError::Refused("Nothing to start a radio from: play something of the library, or name a playlist or an album.".into()));
        }
        let (mode, albums, skip_favourites) = {
            let entries = self.entries();
            let q = entries.get(name).map(|e| &e.queue);
            let a = q.and_then(|q| q.autoplay.as_ref());
            (a.map(|a| a.mode).unwrap_or_default(), a.is_some_and(|a| a.albums), q.is_some_and(|q| q.skip_favourites))
        };
        let ask = super::radio::NextRequest {
            mode: super::radio::Mode::of(mode),
            seed: seed.clone(),
            exclude: seed.clone(),
            last: None,
            count: Some(RADIO_START),
            within: None,
            albums,
            by: RadioBy::default(),
            skip_favourites,
        };
        let first = super::radio::next(ask).await?;
        if first.is_empty() {
            return Err(ToggleError::Refused("Nothing like it was found.".into()));
        }
        let title = from.name.clone().or(from.playlist.clone()).unwrap_or_else(|| name_of(&seeds));
        // Set before the play: a play of Autoplay's tracks only keeps its seed.
        let auto = Command::Autoplay { mode: AutoplayMode::of(Some(mode)), albums: Some(albums), restart: Some(false), seed: Some(seed), within: Some(String::new()) };
        Box::pin(self.command(name, auto)).await?;
        let play = Command::Play {
            from: PlayFrom { name: Some(format!("Radio: {title}")), ..PlayFrom::default() },
            tracks: Some(first),
            index: 0,
            position_ms: 0,
            shuffle: false,
            paused: false,
            queue_id: None,
            queue_name: None,
        };
        Box::pin(self.command(name, play)).await?;
        let target = self.leader_of(name).unwrap_or_else(|| name.to_string());
        if let Some(e) = self.entries().get_mut(&target) {
            e.queue.from = Some("radio".into());
        }
        Ok(())
    }

    /// Queue history (decision 0033, amended): the player takes up its queue `id` where it was left, playing if it
    /// played, else paused there; the one it leaves is kept as it stood.
    async fn switch(&self, name: &str, id: &str) -> Result<(), ToggleError> {
        let kept = {
            let entries = self.entries();
            let Some(e) = entries.get(name) else { return Err(ToggleError::Refused(format!("No player is called {name}."))) };
            if !id.is_empty() && e.queue.id == id {
                return Ok(());
            }
            e.kept.iter().find(|k| k.queue.id == id).cloned()
        };
        let Some(k) = kept else { return Err(ToggleError::Refused("That queue is not kept.".into())) };
        let index = k.entry.as_deref().and_then(|x| k.queue.at(x)).unwrap_or(0);
        let play = Command::Play {
            from: PlayFrom { whole: true, ..PlayFrom::default() },
            tracks: Some(k.queue.tracks.clone()),
            index,
            position_ms: k.position_ms,
            shuffle: false,
            paused: !k.playing,
            queue_id: Some(k.queue.id.clone()),
            queue_name: Some(k.queue.name.clone()),
        };
        Box::pin(self.command(name, play)).await?;
        self.settle(name, &k.queue).await;
        tracing::info!(target: "playon", player = name, queue = k.queue.name.as_str(), "queue taken up again");
        self.changed();
        Ok(())
    }

    /// Queue history by where it was heard (decision 0033, amended 2026-10-19): the newest queue heard on an output or
    /// player `text` names, from this player's list, else from the same account's other players' lists (newest left
    /// first). Played here from where it stood.
    async fn switch_to_played_on(&self, name: &str, text: &str) -> Result<(), ToggleError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(ToggleError::Refused("Played on what?".into()));
        }
        let found = {
            let entries = self.entries();
            let Some(me) = entries.get(name) else { return Err(ToggleError::Refused(format!("No player is called {name}."))) };
            let heard = |k: &&Kept| k.queue.played_on.iter().any(|p| p.named(text));
            let first = first_owner();
            let mine = |e: &Entry| e.owner.as_deref().or(first.as_deref()) == me.owner.as_deref().or(first.as_deref());
            me.kept.iter().find(heard).cloned().or_else(|| {
                entries.iter().filter(|(n, e)| n.as_str() != name && mine(e)).flat_map(|(_, e)| e.kept.iter().filter(heard)).max_by_key(|k| k.updated).cloned()
            })
        };
        let Some(k) = found else { return Err(ToggleError::Refused(format!("No queue played on {text}."))) };
        let index = k.entry.as_deref().and_then(|x| k.queue.at(x)).unwrap_or(0);
        let play = Command::Play {
            from: PlayFrom { whole: true, ..PlayFrom::default() },
            tracks: Some(k.queue.tracks.clone()),
            index,
            position_ms: k.position_ms,
            shuffle: false,
            paused: false,
            queue_id: Some(k.queue.id.clone()),
            queue_name: Some(k.queue.name.clone()),
        };
        Box::pin(self.command(name, play)).await?;
        self.settle(name, &k.queue).await;
        tracing::info!(target: "playon", player = name, queue = k.queue.name.as_str(), played_on = text, "queue taken up again");
        self.changed();
        Ok(())
    }

    /// Queue history: queue `id` (the one that plays too) renamed; kept then until deleted.
    fn rename_queue(&self, name: &str, id: &str, new: &str) -> Result<(), ToggleError> {
        let new: String = new.trim().chars().take(QUEUE_NAME_MAX).collect();
        if new.is_empty() {
            return Err(ToggleError::Refused("A queue needs a name.".into()));
        }
        let mut entries = self.entries();
        let Some(e) = entries.get_mut(name) else { return Err(ToggleError::Refused(format!("No player is called {name}."))) };
        let queue = if !id.is_empty() && e.queue.id == id {
            &mut e.queue
        } else if let Some(k) = e.kept.iter_mut().find(|k| k.queue.id == id) {
            &mut k.queue
        } else {
            return Err(ToggleError::Refused("That queue is not kept.".into()));
        };
        (queue.name, queue.renamed) = (new, true);
        e.kept_version += 1;
        drop(entries);
        self.changed();
        Ok(())
    }

    /// Queue history: queue `id` out of the list; not the one that plays (switch to another first).
    fn delete_queue(&self, name: &str, id: &str) -> Result<(), ToggleError> {
        let mut entries = self.entries();
        let Some(e) = entries.get_mut(name) else { return Err(ToggleError::Refused(format!("No player is called {name}."))) };
        if !id.is_empty() && e.queue.id == id {
            return Err(ToggleError::Refused("That queue plays: switch to another first.".into()));
        }
        let before = e.kept.len();
        e.kept.retain(|k| k.queue.id != id);
        if e.kept.len() == before {
            return Err(ToggleError::Refused("That queue is not kept.".into()));
        }
        e.kept_version += 1;
        drop(entries);
        self.changed();
        Ok(())
    }

    /// A player the music was handed away from without its knowing: its queue emptied here, as a stop does, so it is
    /// not given back when the player returns without one; the entries that moved kept, to stop it should it still
    /// play one of them.
    fn stopped_away(&self, name: &str) {
        let mut entries = self.entries();
        let Some(e) = entries.get_mut(name) else { return };
        e.moved = e.queue.tracks.iter().map(|t| t.entry.clone()).filter(|x| !x.is_empty()).collect();
        let standing = e.standing();
        e.keep(standing);
        if e.queue.edit_at(Command::Stop, None).is_ok() {
            e.told = e.queue.version;
        }
        (e.queue.id, e.queue.name, e.queue.renamed, e.queue.from) = (String::new(), String::new(), false, None);
        e.report = Report { index: -1, volume: e.report.volume, ducked: e.report.ducked, ..Report::default() };
        e.at = Instant::now();
        drop(entries);
        tracing::info!(target: "playon", player = name, "stopped while offline");
        self.changed();
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
        let secret = self.entries().get(name).map(|e| e.secret.clone()).unwrap_or_default();
        let _ = self.send(name, &Command::Group { leader, host, port, buffer_ms, secret });
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
        tracing::info!(target: "playon", player = name, leader = leader.as_str(), "joined a group");
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
        tracing::info!(target: "playon", player = name, leader = leader.as_str(), "left its group");
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
            let was = before.kodi.iter().find(|o| o.name == name);
            let password = if k.password.is_empty() { was.map(|o| o.password.clone()).unwrap_or_default() } else { k.password };
            // Whose device it is: as it was, else the administrator's who adds it.
            let owner = was.and_then(|o| o.owner.clone()).or_else(whose);
            kodi.push(super::kodi::KodiConfig { name, url, user: k.user.trim().to_string(), password, stream_from, owner });
        }
        let saved = PlayOnSettings { webhooks, delays: before.delays, kodi };
        let body = serde_json::to_string_pretty(&saved).map_err(anyhow::Error::from)?;
        tokio::task::spawn_blocking(move || write_private(&file, &body)).await.map_err(|e| anyhow::anyhow!("{e}"))??;
        *self.settings.lock().unwrap_or_else(|p| p.into_inner()) = saved;
        self.start_kodis();
        self.changed();
        Ok(self.settings_view())
    }

    /// The account a player is a device of (the first administrator for one from before accounts).
    pub fn owner_of(&self, name: &str) -> Option<String> {
        self.entries().get(name).and_then(|e| e.owner.clone()).or_else(first_owner)
    }

    /// A player's speaker delay (ms).
    pub fn delay_of(&self, name: &str) -> u16 {
        self.settings.lock().unwrap_or_else(|p| p.into_inner()).delays.get(name).copied().unwrap_or(0)
    }

    /// Sets a player's speaker delay (0 to [`MAX_DELAY_MS`]); heard at once in a group. Saved in `players.json` when
    /// there is one.
    pub async fn set_delay(&self, name: &str, ms: u16) -> Result<u16, ToggleError> {
        valid_name(name)?;
        self.reached(name)?;
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
            // Each webhook's failures in a row, so the log shows when a bad spell began, why, and when it ended.
            let mut failing: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
            loop {
                // Behind (more changes than the channel holds) is a change too; closed is Cratarr stopping.
                if let Err(broadcast::error::RecvError::Closed) = rx.recv().await {
                    return;
                }
                tokio::time::sleep(SETTLE).await;
                while rx.try_recv().is_ok() {}
                let Some(me) = me.upgrade() else { return };
                // Home Assistant's: its key acts for the first administrator, whose players these are (decision 0051);
                // every one while signing in is off.
                let reach = crate::serve::instance().filter(|s| s.sign_in_required()).and_then(|s| s.users.owner());
                let (hooks, players) = (me.settings().webhooks, me.list_for(reach.as_deref()));
                let seq = me.seq.fetch_add(1, Ordering::Relaxed) + 1;
                drop(me);
                if hooks.is_empty() {
                    continue;
                }
                let body = serde_json::json!({ "seq": seq, "players": players });
                for url in hooks {
                    let started = std::time::Instant::now();
                    let sent = http.post(&url).json(&body).send().await.and_then(|r| r.error_for_status());
                    if let Err(e) = sent {
                        let n = failing.entry(url.clone()).or_insert(0);
                        *n += 1;
                        tracing::warn!("Play On webhook {url} failed after {} ms ({n} in a row): {}", started.elapsed().as_millis(), causes(&e));
                    } else if let Some(n) = failing.remove(&url) {
                        tracing::info!("Play On webhook {url} answers again, after {n} failures");
                    }
                }
            }
        });
    }
}

/// An error with each of its causes. reqwest's own message leaves them out: a refused connection, a timeout and a name
/// not found all read "error sending request for url".
fn causes(e: &dyn std::error::Error) -> String {
    let mut out = e.to_string();
    let mut cause = e.source();
    while let Some(c) = cause {
        out.push_str(": ");
        out.push_str(&c.to_string());
        cause = c.source();
    }
    out
}

/// What the Snapcast server asks of Play On (decision 0037).
impl crate::snapcast::Groups for PlayOn {
    fn leader_of(&self, player: &str) -> Option<String> {
        PlayOn::leader_of(self, player)
    }

    fn latency_of(&self, player: &str) -> i32 {
        i32::from(self.delay_of(player))
    }

    fn admits(&self, player: &str, secret: &str) -> bool {
        self.entries().get(player).is_some_and(|e| !secret.is_empty() && same(e.secret.as_bytes(), secret.as_bytes()))
    }

    fn volume_of(&self, player: &str) -> u8 {
        self.entries().get(player).map_or(100, |e| {
            let v = e.report.volume.min(100);
            if e.report.ducked { (f32::from(v) * DUCKED) as u8 } else { v }
        })
    }
}

/// A player's sync play secret: 16 random bytes, as hex.
fn new_secret() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("the operating system's random source");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Equal, in a time that does not tell how much of it is.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
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
/// A command's name as players are told it (`play`, `stop`…), for the log.
fn command_name(c: &Command) -> String {
    serde_json::to_value(c).ok().and_then(|v| v["command"].as_str().map(str::to_string)).unwrap_or_default()
}

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

/// Tracks of Cratarr's own library (decision 0032) for `play`: an album in its order, the favourites of the player's
/// `owner`, or tracks from the whole library at random. `None` when none of those is asked for.
async fn own_tracks(from: &PlayFrom, owner: Option<String>) -> Result<Option<Vec<QueueTrack>>, ToggleError> {
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
            let owner = owner.context("there is no account yet: make one in Settings")?;
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
    fn a_webhook_failure_is_logged_with_its_causes() {
        #[derive(Debug, thiserror::Error)]
        #[error("error sending request for url")]
        struct Sending(#[source] std::io::Error);
        let refused = Sending(std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "connection refused"));
        assert_eq!(causes(&refused), "error sending request for url: connection refused");
        assert_eq!(causes(&std::io::Error::other("plain")), "plain");
    }

    #[test]
    fn commands_read_as_home_assistant_sends_them_and_reach_the_player_resolved() {
        let c: Command = serde_json::from_str(r#"{"command":"play","playlist":"Chill","shuffle":true}"#).unwrap();
        let chill = PlayFrom { playlist: Some("Chill".into()), ..PlayFrom::default() };
        assert_eq!(c, Command::Play { from: chill, tracks: None, index: 0, position_ms: 0, shuffle: true, paused: false, queue_id: None, queue_name: None });
        let c: Command = serde_json::from_str(r#"{"command":"play","library":true,"shuffle":true,"limit":50}"#).unwrap();
        let library = PlayFrom { library: true, limit: Some(50), ..PlayFrom::default() };
        assert_eq!(c, Command::Play { from: library, tracks: None, index: 0, position_ms: 0, shuffle: true, paused: false, queue_id: None, queue_name: None });
        let c: Command = serde_json::from_str(r#"{"command":"play","album":"al-x"}"#).unwrap();
        assert!(matches!(c, Command::Play { from: PlayFrom { album: Some(a), favourites: false, .. }, .. } if a == "al-x"));
        let c: Command = serde_json::from_str(r#"{"command":"seek","percent":50}"#).unwrap();
        assert_eq!(c, Command::Seek { position_ms: None, percent: Some(50.0), by_ms: None });
        assert_eq!(serde_json::from_str::<Command>(r#"{"command":"duck"}"#).unwrap(), Command::Duck);
        // To the player: tracks, never the playlist's name or the shuffle wish.
        let to_player = Command::play(vec![], 2, 5);
        assert_eq!(serde_json::to_string(&to_player).unwrap(), r#"{"command":"play","tracks":[],"index":2,"position_ms":5}"#);
        assert_eq!(serde_json::to_string(&Command::Seek { position_ms: Some(9), percent: None, by_ms: None }).unwrap(), r#"{"command":"seek","position_ms":9}"#);
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
    fn a_player_its_device_holds_back_says_why() {
        let mut e = Entry::new();
        assert_eq!(e.view("p", false).held, None);
        // As the app reports it (decision 0031, amended 2026-10-19); an app from before it says nothing.
        let r: Report = serde_json::from_str(r#"{"index":0,"position_ms":0,"playing":false,"held":"Android would not let it play"}"#).unwrap();
        e.report = r;
        assert_eq!(e.view("p", false).held.as_deref(), Some("Android would not let it play"));
        e.report = serde_json::from_str(r#"{"index":0,"position_ms":0,"playing":true,"held":null}"#).unwrap();
        assert_eq!(e.view("p", false).held, None);
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
    async fn a_player_is_its_accounts_and_one_from_before_the_first_administrators() {
        // A name is one account's player (decision 0051).
        let mut entries = HashMap::new();
        claim(&mut entries, "Phone", Some("ann")).unwrap();
        assert_eq!(entries["Phone"].owner.as_deref(), Some("ann"));
        assert!(claim(&mut entries, "Phone", Some("bob")).is_err(), "a name is one player's");
        claim(&mut entries, "Phone", None).unwrap();
        assert_eq!(entries["Phone"].owner.as_deref(), Some("ann"), "Cratarr's own work makes no one its owner");
        let old = Entry::new();
        assert!(belongs_to(&old, "me", Some("me")) && !belongs_to(&old, "bob", Some("me")), "from before accounts: the first administrator's");

        let p = play_on().await;
        let _phone = p.connect("Phone").unwrap();
        let tablet = p.connect("Tablet").unwrap();
        p.entries().get_mut("Phone").unwrap().owner = Some("ann".into());
        p.entries().get_mut("Tablet").unwrap().owner = Some("bob".into());
        let names = |user: Option<&str>| p.list_for(user).into_iter().map(|v| v.name).collect::<Vec<_>>();
        assert_eq!(names(Some("ann")), ["Phone"]);
        assert_eq!(names(Some("bob")), ["Tablet"]);
        assert_eq!(names(None), ["Phone", "Tablet"]);
        // One's own devices, removed: not while connected, nor another's.
        assert!(p.forget("Tablet", "bob").is_err(), "connected");
        drop(tablet);
        assert!(!p.forget("Tablet", "ann").unwrap(), "not hers");
        assert!(p.forget("Tablet", "bob").unwrap());
        assert_eq!(names(None), ["Phone"]);
    }

    #[tokio::test]
    async fn a_browser_is_a_player_while_open_and_never_in_a_group() {
        let p = play_on().await;
        let t = |id: &str| QueueTrack { source: "cratarr".into(), id: id.into(), name: id.into(), ..QueueTrack::default() };
        p.serve_sync_play(1704, 1000, None);
        let mut page = p.connect_browser("Chrome on Android", Some("tab-1")).unwrap();
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
        let mut again = p.connect_browser("Chrome on Android", Some("tab-1")).unwrap();
        // Another tab may not take that name while this one holds it, nor a name the app holds.
        assert!(matches!(p.connect_browser("Chrome on Android", Some("tab-2")), Err(ToggleError::Refused(_))));
        assert!(matches!(p.connect_browser("tablet", Some("tab-2")), Err(ToggleError::Refused(_))));
        said(&mut again);
        p.report("Chrome on Android", Report { index: -1, ..Report::default() }).unwrap();
        let back = told(&mut again);
        let play = back.iter().find(|m| m["command"] == "play").expect("its queue given back");
        assert_eq!((play["tracks"].as_array().unwrap().len(), play["index"].as_u64()), (2, Some(1)));
    }

    #[tokio::test]
    async fn a_seek_by_moves_on_or_back_from_where_it_is() {
        let p = play_on().await;
        let t = |id: &str| QueueTrack { source: "cratarr".into(), id: id.into(), name: id.into(), ..QueueTrack::default() };
        let mut tablet = p.connect("tablet").unwrap();
        p.command("tablet", Command::play(vec![t("tr-1")], 0, 0)).await.unwrap();
        said(&mut tablet);
        p.report("tablet", Report { index: 0, position_ms: 60_000, duration_ms: Some(200_000), playing: false, ..Report::default() }).unwrap();
        let sought = |c: &mut Commands| told(c).iter().filter(|m| m["command"] == "seek").filter_map(|m| m["position_ms"].as_u64()).last();
        let by = |ms: i64| Command::Seek { position_ms: None, percent: None, by_ms: Some(ms) };
        p.command("tablet", by(10_000)).await.unwrap();
        assert_eq!(sought(&mut tablet), Some(70_000), "on from where it is");
        p.command("tablet", by(-90_000)).await.unwrap();
        assert_eq!(sought(&mut tablet), Some(0), "never before the start");
        p.command("tablet", by(500_000)).await.unwrap();
        assert_eq!(sought(&mut tablet), Some(199_000), "short of the end by a second");
        // As the page sends it; the player is told a position only.
        let c: Command = serde_json::from_str(r#"{"command":"seek","by_ms":-30000}"#).unwrap();
        assert_eq!(c, by(-30_000));
    }

    #[tokio::test]
    async fn a_player_given_its_queue_back_is_not_emptied_by_what_it_said_before_it_had_it() {
        let p = play_on().await;
        let t = |id: &str| QueueTrack { source: "cratarr".into(), id: id.into(), name: id.into(), ..QueueTrack::default() };
        let tablet = p.connect("tablet").unwrap();
        p.command("tablet", Command::play(vec![t("tr-1"), t("tr-2"), t("tr-3")], 1, 0)).await.unwrap();
        let version = p.get("tablet").unwrap().queue_version;
        p.report("tablet", Report { index: 1, version: Some(version), ..Report::default() }).unwrap();

        // Its app is updated: back without a queue, it is given Cratarr's, and reports once more before that reaches it.
        drop(tablet);
        let mut tablet = p.connect("tablet").unwrap();
        said(&mut tablet);
        p.report("tablet", Report { index: -1, ..Report::default() }).unwrap();
        let back = told(&mut tablet);
        let play = back.iter().find(|m| m["command"] == "play").expect("its queue given back");
        p.report("tablet", Report { index: -1, queue: Some(Vec::new()), ..Report::default() }).unwrap();
        let v = p.get("tablet").unwrap();
        assert_eq!((v.queue_length, v.index), (3, 1), "not emptied, and still where it was");
        // Nor with the version its group came with as it connected (the tablet at 00:36, 2026-10-18): the queue given
        // back has a version of its own.
        assert!(play["version"].as_u64() > Some(version));
        p.report("tablet", Report { index: -1, queue: Some(Vec::new()), version: Some(version), ..Report::default() }).unwrap();
        let v = p.get("tablet").unwrap();
        assert_eq!((v.queue_length, v.index), (3, 1), "not emptied by a report with the group's version");

        // Once it has it, its reports count again: the queue as it holds it, emptied too.
        p.report("tablet", Report { index: -1, queue: Some(Vec::new()), version: play["version"].as_u64(), ..Report::default() }).unwrap();
        assert_eq!(p.get("tablet").unwrap().queue_length, 0);
    }

    #[tokio::test]
    async fn a_new_play_keeps_the_queue_it_leaves_and_a_switch_takes_it_up_where_it_was() {
        let p = play_on().await;
        let t = |id: &str| QueueTrack { source: "cratarr".into(), id: id.into(), name: id.into(), album: Some("Transit".into()), ..QueueTrack::default() };
        let mut tablet = p.connect("tablet").unwrap();
        p.command("tablet", Command::play(vec![t("tr-1"), t("tr-2"), t("tr-3")], 1, 0)).await.unwrap();
        let first = p.get("tablet").unwrap().queues.unwrap();
        assert_eq!((first.len(), first[0].name.as_str(), first[0].playing, first[0].from.as_deref()), (1, "Transit", true, Some("tracks")));
        let version = p.get("tablet").unwrap().queue_version;
        p.report("tablet", Report { index: 1, version: Some(version), position_ms: 40_000, playing: true, ..Report::default() }).unwrap();

        // Something new, by name: the one before is kept whole, where it stood.
        let mut chill = Command::play(vec![t("tr-7")], 0, 0);
        if let Command::Play { from, .. } = &mut chill {
            from.name = Some("Chill".into());
        }
        p.command("tablet", chill).await.unwrap();
        let list = p.get("tablet").unwrap().queues.unwrap();
        assert_eq!(list.iter().map(|q| (q.name.as_str(), q.playing, q.tracks)).collect::<Vec<_>>(), vec![("Chill", true, 1), ("Transit", false, 3)]);
        let told_play = told(&mut tablet).into_iter().rfind(|m| m["command"] == "play").unwrap();
        assert_eq!(told_play["queue_name"], "Chill", "the player is told the queue's name and id");
        assert_eq!(told_play["queue_id"].as_str(), Some(list[0].id.as_str()));

        // Taken up again: from its track and position, playing as it played; Chill kept in its place.
        let transit = list[1].id.clone();
        p.command("tablet", Command::Queue { id: transit.clone(), played_on: None }).await.unwrap();
        let play = told(&mut tablet).into_iter().find(|m| m["command"] == "play").unwrap();
        assert_eq!((play["index"].as_u64(), play["position_ms"].as_u64(), play["paused"].as_bool()), (Some(1), Some(40_000), None));
        assert_eq!(play["tracks"].as_array().map(Vec::len), Some(3), "whole, nothing of Chill's brought along");
        let list = p.get("tablet").unwrap().queues.unwrap();
        assert_eq!(list.iter().map(|q| (q.name.as_str(), q.playing)).collect::<Vec<_>>(), vec![("Transit", true), ("Chill", false)]);
        assert_eq!(list[0].id, transit);

        // Renamed (the playing one too) and deleted (not the playing one).
        p.command("tablet", Command::QueueRename { id: list[1].id.clone(), name: "  Evening  ".into() }).await.unwrap();
        assert!(matches!(p.command("tablet", Command::QueueDelete { id: transit }).await, Err(ToggleError::Refused(_))));
        let evening = p.get("tablet").unwrap().queues.unwrap()[1].clone();
        assert_eq!(evening.name, "Evening");
        p.command("tablet", Command::QueueDelete { id: evening.id }).await.unwrap();
        assert_eq!(p.get("tablet").unwrap().queues.unwrap().len(), 1);
        // A stop keeps it too.
        p.command("tablet", Command::Stop).await.unwrap();
        let list = p.get("tablet").unwrap().queues.unwrap();
        assert_eq!(list.iter().map(|q| (q.name.as_str(), q.playing)).collect::<Vec<_>>(), vec![("Transit", false)]);
    }

    #[tokio::test]
    async fn twenty_queues_a_player_the_oldest_not_renamed_going_first() {
        let p = play_on().await;
        let t = |id: &str| QueueTrack { source: "cratarr".into(), id: id.into(), name: id.into(), ..QueueTrack::default() };
        let mut tablet = p.connect("tablet").unwrap();
        p.command("tablet", Command::play(vec![t("first")], 0, 0)).await.unwrap();
        p.command("tablet", Command::play(vec![t("second")], 0, 0)).await.unwrap();
        let first = p.get("tablet").unwrap().queues.unwrap()[1].id.clone();
        p.command("tablet", Command::QueueRename { id: first, name: "Keep".into() }).await.unwrap();
        for i in 0..30 {
            p.command("tablet", Command::play(vec![t(&format!("tr-{i}"))], 0, 0)).await.unwrap();
            said(&mut tablet);
        }
        let list = p.get("tablet").unwrap().queues.unwrap();
        assert_eq!(list.len(), QUEUES);
        assert_eq!(list[0].name, "tr-29");
        assert!(list.iter().any(|q| q.name == "Keep"), "a renamed one stays");
        assert!(!list.iter().any(|q| q.name == "second"), "the oldest not renamed went");
    }

    #[tokio::test]
    async fn a_queue_the_player_starts_itself_or_empties_keeps_the_one_before() {
        let p = play_on().await;
        let t = |id: &str| QueueTrack { source: "cratarr".into(), id: id.into(), name: id.into(), artist: Some("Skalpel".into()), ..QueueTrack::default() };
        let _tablet = p.connect("tablet").unwrap();
        p.command("tablet", Command::play(vec![t("tr-1"), t("tr-2")], 0, 0)).await.unwrap();
        let version = p.get("tablet").unwrap().queue_version;
        p.report("tablet", Report { index: 0, version: Some(version), ..Report::default() }).unwrap();

        // Its own, named: the page in the app played an album there.
        let own = vec![QueueTrack { entry: "e-9".into(), ..t("tr-9") }];
        let named = Report { index: 0, version: Some(version), queue: Some(own), queue_id: Some("q-own".into()), queue_name: Some("Sleep".into()), ..Report::default() };
        p.report("tablet", named).unwrap();
        let list = p.get("tablet").unwrap().queues.unwrap();
        assert_eq!(list.iter().map(|q| (q.id.as_str(), q.name.as_str(), q.playing)).collect::<Vec<_>>()[0], ("q-own", "Sleep", true));
        assert_eq!((list[1].name.as_str(), list[1].tracks), ("Skalpel", 2), "the one before, named after its artist");
        // Emptied there (an app from before names none): kept too, and nothing plays.
        p.report("tablet", Report { index: -1, version: Some(version), queue: Some(Vec::new()), ..Report::default() }).unwrap();
        let list = p.get("tablet").unwrap().queues.unwrap();
        assert_eq!(list.iter().map(|q| (q.name.as_str(), q.playing)).collect::<Vec<_>>(), vec![("Sleep", false), ("Skalpel", false)]);
    }

    #[tokio::test]
    async fn a_handover_moves_the_queue_as_itself_and_the_player_it_left_keeps_it() {
        let p = play_on().await;
        let t = |id: &str| QueueTrack { source: "cratarr".into(), id: id.into(), name: id.into(), ..QueueTrack::default() };
        let (_phone, mut tablet) = (p.connect("phone").unwrap(), p.connect("tablet").unwrap());
        let mut car = Command::play(vec![t("tr-1"), t("tr-2")], 0, 0);
        if let Command::Play { from, .. } = &mut car {
            from.name = Some("Car".into());
        }
        p.command("phone", car).await.unwrap();
        let id = p.get("phone").unwrap().queues.unwrap()[0].id.clone();
        p.command("phone", Command::Handover { to: "tablet".into() }).await.unwrap();
        let play = told(&mut tablet).into_iter().find(|m| m["command"] == "play").unwrap();
        assert_eq!((play["queue_id"].as_str(), play["queue_name"].as_str()), (Some(id.as_str()), Some("Car")));
        let phone = p.get("phone").unwrap().queues.unwrap();
        assert_eq!(phone.iter().map(|q| (q.id.as_str(), q.playing)).collect::<Vec<_>>(), vec![(id.as_str(), false)]);
        // A browser keeps its own list (in the page), none here.
        let _page = p.connect_browser("Chrome on Android", Some("tab-1")).unwrap();
        p.command("Chrome on Android", Command::play(vec![t("tr-1")], 0, 0)).await.unwrap();
        p.command("Chrome on Android", Command::play(vec![t("tr-2")], 0, 0)).await.unwrap();
        assert_eq!(p.get("Chrome on Android").unwrap().queues.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn home_assistant_adds_by_name_starts_a_radio_and_has_the_queues_pushed() {
        // Add by name, as play takes them; told to the player as tracks only.
        let c: Command = serde_json::from_str(r#"{"command":"add","playlist":"Chill","how":"next"}"#).unwrap();
        assert!(matches!(&c, Command::Add { from: PlayFrom { playlist: Some(p), .. }, tracks, .. } if p == "Chill" && tracks.is_empty()));
        let c: Command = serde_json::from_str(r#"{"command":"play","radio":true,"album":"al-x"}"#).unwrap();
        assert!(matches!(&c, Command::Play { from: PlayFrom { radio: true, album: Some(a), .. }, .. } if a == "al-x"));

        let p = play_on().await;
        let t = |id: &str| QueueTrack { source: "cratarr".into(), id: id.into(), name: id.into(), ..QueueTrack::default() };
        let _tablet = p.connect("tablet").unwrap();
        let nothing: Command = serde_json::from_str(r#"{"command":"add","how":"last"}"#).unwrap();
        assert!(matches!(p.command("tablet", nothing).await, Err(ToggleError::Refused(_))), "nothing named, nothing given");
        // A radio from what plays: nothing plays.
        let radio: Command = serde_json::from_str(r#"{"command":"play","radio":true}"#).unwrap();
        assert!(matches!(p.command("tablet", radio).await, Err(ToggleError::Refused(m)) if m.starts_with("Nothing to start a radio from")));

        // The list goes out with every player, as the stream and the webhook send them.
        p.command("tablet", Command::play(vec![t("tr-1")], 0, 0)).await.unwrap();
        p.command("tablet", Command::play(vec![t("tr-2")], 0, 0)).await.unwrap();
        let all = p.list_for(None);
        let tablet = all.iter().find(|v| v.name == "tablet").unwrap();
        assert_eq!(tablet.queues.as_ref().map(Vec::len), Some(2));
        assert!(tablet.queue.is_none(), "the tracks themselves only by name");
    }

    #[tokio::test]
    async fn a_queue_remembers_where_it_was_heard_and_is_found_by_it() {
        let p = play_on().await;
        let t = |id: &str| QueueTrack { source: "cratarr".into(), id: id.into(), name: id.into(), ..QueueTrack::default() };
        let (_phone, mut tablet) = (p.connect("Phone").unwrap(), p.connect("Salon tablet").unwrap());
        let car = Output { kind: "bluetooth".into(), name: Some("JVC KW-M560BT".into()) };
        // The phone in the car: its output said, and a play of no playlist or album named after the car's device.
        p.report("Phone", Report { index: -1, output: Some(car.clone()), ..Report::default() }).unwrap();
        assert_eq!(p.get("Phone").unwrap().output, Some(car.clone()));
        p.command("Phone", Command::play(vec![t("tr-1"), t("tr-2")], 0, 0)).await.unwrap();
        let version = p.get("Phone").unwrap().queue_version;
        p.report("Phone", Report { index: 0, version: Some(version), playing: true, output: Some(car.clone()), ..Report::default() }).unwrap();
        let q = p.get("Phone").unwrap().queues.unwrap().remove(0);
        assert_eq!(q.name, "JVC KW-M560BT");
        assert_eq!(q.played_on, vec![PlayedOn { player: "Phone".into(), kind: Some("bluetooth".into()), name: Some("JVC KW-M560BT".into()) }]);

        // Home: the phone's queue goes to the salon and is heard there too; the phone keeps it as it stood.
        p.command("Phone", Command::Handover { to: "Salon tablet".into() }).await.unwrap();
        let version = p.get("Salon tablet").unwrap().queue_version;
        p.report("Salon tablet", Report { index: 0, version: Some(version), playing: true, ..Report::default() }).unwrap();
        let heard: Vec<String> = p.get("Salon tablet").unwrap().queues.unwrap()[0].played_on.iter().map(|o| o.player.clone()).collect();
        assert_eq!(heard, ["Phone", "Salon tablet"], "where it was heard moves with it");
        // Something else in the salon; later "the car queue again": found by the car's device, played from where it was.
        p.command("Salon tablet", Command::play(vec![t("tr-9")], 0, 0)).await.unwrap();
        said(&mut tablet);
        let again = Command::Queue { id: String::new(), played_on: Some("jvc".into()) };
        p.command("Salon tablet", again).await.unwrap();
        let play = told(&mut tablet).into_iter().find(|m| m["command"] == "play").unwrap();
        assert_eq!((play["queue_name"].as_str(), play["paused"].as_bool()), (Some("JVC KW-M560BT"), None), "played");
        let nowhere = Command::Queue { id: String::new(), played_on: Some("kitchen".into()) };
        assert!(matches!(p.command("Salon tablet", nowhere).await, Err(ToggleError::Refused(m)) if m == "No queue played on kitchen."));
    }

    #[tokio::test]
    async fn a_device_in_local_mode_is_seen_and_steered_but_its_queue_is_not_cratarrs_to_change() {
        let p = play_on().await;
        let t = |id: &str| QueueTrack { source: "cratarr".into(), id: id.into(), name: id.into(), ..QueueTrack::default() };
        let mut phone = p.connect("Phone").unwrap();
        p.command("Phone", Command::play(vec![t("tr-1"), t("tr-2")], 1, 0)).await.unwrap();
        let version = p.get("Phone").unwrap().queue_version;
        p.report("Phone", Report { index: 1, version: Some(version), playing: true, ..Report::default() }).unwrap();
        said(&mut phone);

        // Local mode (decision 0052): its own music shows; Cratarr's queue waits as it was.
        let own = DeviceTrack { name: "Demo take".into(), artist: Some("Me".into()), album: None };
        let local = Report { mode: Some("device".into()), device_track: Some(own), playing: true, position_ms: 5_000, index: -1, ..Report::default() };
        p.report("Phone", local).unwrap();
        let v = p.get("Phone").unwrap();
        assert_eq!((v.mode, v.title.as_deref(), v.artist.as_deref(), v.state, v.queue_length, v.index), ("device", Some("Demo take"), Some("Me"), "playing", 0, -1));
        assert!(v.track.is_none());
        // Steered: a pause reaches it; a stop is a pause there; an edit or a handover from it is refused.
        p.command("Phone", Command::Pause).await.unwrap();
        p.command("Phone", Command::Stop).await.unwrap();
        assert_eq!(said(&mut phone), ["pause", "pause"]);
        let add = Command::Add { from: Default::default(), tracks: vec![t("tr-9")], at: None, after: None, how: None };
        assert!(matches!(p.command("Phone", add).await, Err(ToggleError::Refused(m)) if m == OWN_MUSIC));
        let _tablet = p.connect("tablet").unwrap();
        assert!(matches!(p.command("Phone", Command::Handover { to: "tablet".into() }).await, Err(ToggleError::Refused(m)) if m == OWN_MUSIC));

        // Back to Cratarr's music: its queue as it was.
        p.report("Phone", Report { index: 1, version: Some(version), playing: true, ..Report::default() }).unwrap();
        let v = p.get("Phone").unwrap();
        assert_eq!((v.mode, v.queue_length, v.index), ("cratarr", 2, 1));
    }

    #[test]
    fn kept_queues_are_saved_with_the_queue() {
        let s: Saved = serde_json::from_str(
            r#"{"version":3,"tracks":[],"kept":[{"version":1,"tracks":[{"source":"cratarr","id":"tr-1","name":"One"}],"id":"q1","name":"Morning","renamed":true,"entry":"e1","position_ms":5,"playing":true,"updated":9}]}"#,
        )
        .unwrap();
        assert_eq!((s.kept[0].queue.name.as_str(), s.kept[0].queue.renamed, s.kept[0].position_ms), ("Morning", true, 5));
        let again: Saved = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(again.kept, s.kept);
    }

    #[tokio::test]
    async fn a_player_back_without_its_queue_before_it_connects_still_gets_it_back() {
        let p = play_on().await;
        let t = |id: &str| QueueTrack { source: "cratarr".into(), id: id.into(), name: id.into(), ..QueueTrack::default() };
        let tablet = p.connect("tablet").unwrap();
        p.command("tablet", Command::play(vec![t("tr-1"), t("tr-2"), t("tr-3")], 2, 0)).await.unwrap();
        let version = p.get("tablet").unwrap().queue_version;
        p.report("tablet", Report { index: 2, version: Some(version), playing: true, ..Report::default() }).unwrap();

        // Its app is updated and starts again by itself: it reports nothing queued before its connection is open (the
        // tablet at 13:23, 2026-10-19), and that is not taken.
        drop(tablet);
        p.report("tablet", Report { index: -1, queue: Some(Vec::new()), ..Report::default() }).unwrap();
        assert_eq!(p.get("tablet").unwrap().queue_length, 3, "not emptied before it is connected");
        let mut tablet = p.connect("tablet").unwrap();
        said(&mut tablet);
        p.report("tablet", Report { index: -1, queue: Some(Vec::new()), ..Report::default() }).unwrap();
        let back = told(&mut tablet);
        let play = back.iter().find(|m| m["command"] == "play").expect("its queue given back once connected");
        assert_eq!((play["index"].as_u64(), play["tracks"].as_array().map(Vec::len)), (Some(2), Some(3)));
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

    #[tokio::test]
    async fn a_player_handed_away_from_while_offline_does_not_play_that_music_again() {
        let p = play_on().await;
        let t = |id: &str| QueueTrack { source: "cratarr".into(), id: id.into(), name: id.into(), ..QueueTrack::default() };
        let mut tablet = p.connect("tablet").unwrap();
        let phone = p.connect("phone").unwrap();
        p.command("phone", Command::play(vec![t("tr-1"), t("tr-2")], 0, 0)).await.unwrap();
        let entries: Vec<String> = p.get("phone").unwrap().queue.unwrap().iter().map(|t| t.entry.clone()).collect();
        let version = p.get("phone").unwrap().queue_version;
        let playing = |entry: &str| Report { index: 0, entry: Some(entry.into()), version: Some(version), playing: true, ..Report::default() };
        p.report("phone", playing(&entries[0])).unwrap();

        // The phone drops off; the music is handed over from it all the same (Cratarr holds its queue).
        drop(phone);
        p.command("phone", Command::Handover { to: "tablet".into() }).await.unwrap();
        assert_eq!(said(&mut tablet).iter().filter(|c| *c == "play").count(), 1);
        let v = p.get("phone").unwrap();
        assert_eq!((v.state, v.playing, v.queue_length), ("stopped", false, 0), "stopped here, as the stop would have");

        // Back without a queue: nothing is given back.
        let mut phone = p.connect("phone").unwrap();
        p.report("phone", Report { index: -1, ..Report::default() }).unwrap();
        assert!(!said(&mut phone).iter().any(|c| c == "play" || c == "stop"));

        // Back still playing what moved: it is stopped.
        p.command("phone", Command::play(vec![t("tr-3")], 0, 0)).await.unwrap();
        let moved = p.get("phone").unwrap().queue.unwrap()[0].entry.clone();
        drop(phone);
        p.command("phone", Command::Handover { to: "tablet".into() }).await.unwrap();
        let mut phone = p.connect("phone").unwrap();
        said(&mut phone);
        p.report("phone", playing(&moved)).unwrap();
        assert_eq!(said(&mut phone), vec!["stop"]);

        // Back playing something started on it since: that plays on.
        p.command("phone", Command::play(vec![t("tr-4")], 0, 0)).await.unwrap();
        drop(phone);
        p.command("phone", Command::Handover { to: "tablet".into() }).await.unwrap();
        let mut phone = p.connect("phone").unwrap();
        said(&mut phone);
        p.report("phone", playing("chosen-on-the-phone")).unwrap();
        assert!(said(&mut phone).is_empty());
    }

    #[tokio::test]
    async fn a_players_link_shows_how_its_connection_ended() {
        let p = play_on().await;
        let first = p.connect_app("phone", Some("c-1")).unwrap();
        assert_eq!(p.get("phone").unwrap().link, LinkView::default());
        let second = p.connect_app("phone", Some("c-2")).unwrap();
        assert_eq!(p.get("phone").unwrap().link.ended, Some("replaced"));
        // The replaced connection closing changes nothing.
        drop(first);
        assert!(p.get("phone").unwrap().online);
        drop(second);
        let v = p.get("phone").unwrap();
        assert_eq!((v.online, v.link.ended), (false, Some("closed")));
        assert!(v.link.offline_since.is_some() && v.link.ended_at.is_some());
        // Its own view, while its reports get through.
        let own = ReportedLink { failed: 3, last_error: Some("HTTP 401".into()), wait_why: Some("refused".into()), ..ReportedLink::default() };
        p.report("phone", Report { index: -1, link: Some(own.clone()), ..Report::default() }).unwrap();
        assert_eq!(p.get("phone").unwrap().link.own, Some(own));
        // Cratarr stopping: that is why the connections end.
        let _third = p.connect_app("phone", None).unwrap();
        assert_eq!(p.get("phone").unwrap().link.offline_since, None);
        p.stopping();
        assert_eq!(p.get("phone").unwrap().link.ended, Some("server stopping"));
    }

    #[test]
    fn names_that_cannot_be_a_player() {
        assert!(valid_name("Salon tablet").is_ok());
        assert!(valid_name(" ").is_err());
        assert!(valid_name("a\nb").is_err());
        assert!(valid_name(&"x".repeat(65)).is_err());
    }
}
