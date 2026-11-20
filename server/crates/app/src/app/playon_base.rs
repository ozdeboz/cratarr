//! `playon`'s views, in base: a track as players hold it (`QueueTrack`), what can be asked of a player (`Command`) and a
//! player as Home Assistant and the pages see it (`PlayerView`), with the parts they hold. Base's describe and Cratarr's
//! own source name them; `playon` re-exports them.

use serde::{Deserialize, Serialize};

use cratarr_rules::radio::{Mode as RadioMode, RadioBy};

use super::along_base::FollowTarget;
use super::queue_base::{PlayedOn, Repeat};
use super::radio_base::{AutoMode, AutoplayMode, Within};

/// A track as players hold it: the page's own description (`LocalTrack` in web/src/lib/localPlayer.svelte.ts).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct QueueTrack {
    /// Its record's type, as the Library list serves it (GOTO.S1): a queue row's Go to names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub album_type: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(test, ts(as = "Option<Vec<String>>", optional))]
    pub album_secondary: Vec<String>,
    /// Its entry in the queue (decision 0033): an id of its own, never reused, apart from the track's.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub entry: String,
    /// `next`: added by hand (Play next, Add to queue), not part of the album or playlist that plays (0033's lanes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub lane: Option<String>,
    /// Its place in the album or playlist as it was played, before any shuffle: turning shuffle off orders by it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
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
    #[cfg_attr(test, ts(optional))]
    pub duration_secs: Option<f64>,
    /// Who asked for it, a queue request accepted (decision 0079; 0.10): the follower's name, or the guest's as the
    /// page shows it ("Guest Sam"). It stays with the entry wherever the entry goes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub asked_by: Option<String>,
    /// The copy it is heard from, a library's id: Play from's queue names it on every track and every stream of it asks for it,
    /// so it lives as long as the queue does, and a track added names none. Nothing else keeps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub from: Option<String>,
}

/// An announcement as a player reports it (Command::Announce).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Announcing {
    /// The command's id.
    pub id: String,
    /// `playing` from its first sound, `done` once its last sample is heard, `failed` when it could not be played.
    pub state: AnnounceState,
    /// Why it failed, in a few words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub error: Option<String>,
    /// When its parts happened, as the app measured them (0.9.1); apps from before it leave it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub timeline: Option<Timeline>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum AnnounceState {
    Playing,
    Done,
    Failed,
}

/// When an announcement's parts happened, in ms after the player was told (0.9.1; the owner, 2026-10-24: "we need
/// better instrumentation", after a duck that came back ten seconds late): the clip's length, its first sound, the
/// music fully ducked, the speech's end (its last loud block), the music back up, and done; `gave_up` when the clip
/// was given up (the music stopped under it). Each as far as it came.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Timeline {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub clip_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub started_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub ducked_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub speech_end_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub released_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub done_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gave_up: bool,
}

impl Timeline {
    /// How long after the speech's end the music was back up: the duck's exit, the figure to tune by.
    pub fn release_after_speech_ms(&self) -> Option<i64> {
        Some(self.released_ms? as i64 - self.speech_end_ms? as i64)
    }

    /// Its parts in a line, as far as they came.
    fn summary(&self) -> String {
        let parts: Vec<String> = [
            ("clip", self.clip_ms),
            ("started", self.started_ms),
            ("ducked", self.ducked_ms),
            ("speech ended", self.speech_end_ms),
            ("released", self.released_ms),
            ("done", self.done_ms),
        ]
        .into_iter()
        .filter_map(|(name, at)| Some(format!("{name} {} ms", at?)))
        .collect();
        let mut line = parts.join(", ");
        if let Some(late) = self.release_after_speech_ms() {
            line.push_str(&format!("; the music back up {late} ms after the speech"));
        }
        if self.gave_up {
            line.push_str("; given up");
        }
        line
    }
}

impl Announcing {
    /// One line for the log as it ends: its id, how it ended and why, and its timeline.
    pub fn summary(&self) -> String {
        let how = match self.state {
            AnnounceState::Playing => "playing",
            AnnounceState::Done => "done",
            AnnounceState::Failed => "failed",
        };
        let mut line = format!("announcement {} {how}", self.id);
        if let Some(e) = &self.error {
            line.push_str(&format!(" ({e})"));
        }
        if let Some(t) = &self.timeline {
            line.push_str(&format!(": {}", t.summary()));
        }
        line
    }
}

/// Where a hold lands, as the player has it (0.11.1: a driven player's wind shown as it is, 2026-10-30): the track's
/// place in the queue, the place in it, and the hold's top speed (×) where it says it. A page driving the player shows
/// its look-ahead and the hold's zoom by it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Landing {
    pub index: i64,
    pub position_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub top_x: Option<f64>,
}

/// A device's audio output (decision 0033, amended 2026-10-19): `bluetooth`, `speaker`, `wired`, `usb`, `hdmi`,
/// `remote` or `other`, and the device's own name where it has one (a car's Bluetooth, say; none for a speaker).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "AudioOutput"))]
pub struct Output {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub name: Option<String>,
}

impl Output {
    /// A queue's name from where it plays: a Bluetooth device's own (a car's), else none.
    pub(super) fn queue_name(&self) -> Option<String> {
        (self.kind == "bluetooth").then(|| self.name.clone()).flatten().filter(|n| !n.trim().is_empty())
    }
}

/// A player's link as the player sees it (decision 0045): what Cratarr cannot see while the player's reports still get
/// through and its command stream does not. Times are epoch ms on Cratarr's clock.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ReportedLink {
    /// Its command stream open since; none while it is not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub connected_since: Option<u64>,
    /// Attempts to connect that failed since it was last connected.
    #[serde(default)]
    pub failed: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub last_error: Option<String>,
    /// The address of Cratarr's it uses (home or public).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub address: Option<String>,
    /// A wait it is in before trying again: until when, and why (a refusal's ten minutes, say).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub wait_until: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub wait_why: Option<String>,
}

/// A player's link as Cratarr knows it (decision 0045), in its view.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "PlayerLink"))]
pub struct LinkView {
    /// Offline since (epoch seconds); none while online, or not known (offline since Cratarr started).
    pub offline_since: Option<u64>,
    /// How its last connection ended (`replaced`, `gone quiet`, `closed`, `server stopping`), and when (epoch seconds).
    #[cfg_attr(test, ts(type = "\"replaced\" | \"gone quiet\" | \"closed\" | \"server stopping\" | null"))]
    pub ended: Option<&'static str>,
    pub ended_at: Option<u64>,
    /// Its own view, from its last report.
    pub own: Option<ReportedLink>,
}

/// A sleep timer: the player pauses at `at_ms` (epoch milliseconds), or when the playing track ends. The player that
/// plays keeps it, as only it is sure to be awake then (offline, behind a locked screen).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Sleep {
    #[serde(default)]
    pub at_ms: Option<u64>,
    #[serde(default)]
    pub end_of_track: bool,
}

/// A value that is there, null included: `Some(None)` for null, the field's absence stays `None` (its default).
pub(super) fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

/// What `play` plays when it is not given tracks; resolved here, so it never reaches the player.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
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

fn yes() -> bool {
    true
}

/// What can be asked of a player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "command", rename_all = "snake_case")]
#[cfg_attr(test, ts(export, rename = "PlayerCommand"))]
pub enum Command {
    /// A playlist, an album, the favourites, the library or tracks, from track `index`, `position_ms` into it; shuffled
    /// first when asked (then from the start). What reaches the player is always `tracks`.
    Play {
        #[serde(flatten)]
        from: PlayFrom,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
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
        #[cfg_attr(test, ts(optional))]
        queue_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
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
        #[cfg_attr(test, ts(optional))]
        at: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        after: Option<String>,
        #[serde(default, skip_serializing)]
        how: Option<String>,
        /// Played at once (the owner, 2026-11-02: one track plays now): the first added plays now, the queue kept. The
        /// one it cuts counts as played (it ended, not skipped) and the queue goes on after the added ones; Radio and
        /// Autoplay go on as they were (no new seed). Nothing playing: the first added plays.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        #[cfg_attr(test, ts(as = "Option<bool>", optional))]
        now: bool,
    },
    /// Entry `entry` (or the one at place `from`) to after entry `after` ("" for first), or to place `to`.
    Move {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        from: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        to: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        entry: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        after: Option<String>,
    },
    /// Everything after the playing track replaced by these; the playing track plays on.
    Upcoming {
        tracks: Vec<QueueTrack>,
        /// Whether they come shuffled now, when that changed (the shuffle switch).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        shuffled: Option<bool>,
    },
    /// Shuffle, once (the owner, 2026-10-23: a queue always has an order; shuffle is an action, not a state): the tracks
    /// after the playing one mixed as the rules say (cratarr-rules: what was added by hand, Radio's inserts and
    /// Autoplay's tail stay where they are). `on` false (Home Assistant's switch turned off, a page from before) has no
    /// state to turn off: the order stays. A player is told the upcoming tracks, never a shuffle state. Answered with
    /// its Undo (`restore`).
    Shuffle {
        #[serde(default = "yes")]
        on: bool,
    },
    /// The tracks after the playing one, out; the playing track plays on. Answered with its Undo (`restore`). A player is
    /// told the upcoming tracks (none).
    Clear,
    /// Undo of a shuffle or a clear: the tracks that were to come, back (less those played since), in place of what is
    /// to come now (Autoplay's, meanwhile). A player is told the upcoming tracks.
    Restore { tracks: Vec<QueueTrack> },
    /// Repeat off, one (the playing track) or all (the album or playlist), decision 0033 amended.
    Repeat { mode: Repeat },
    /// Radio on or off (decision 0036, amended): while on, the player inserts `count` tracks like each own track
    /// (`by` the track, its artist or its era) right after it; off, or another `by` or `count` while on, takes the
    /// inserts to come out. Left out: as it was. With a mode, seed or bound it is the command from before the
    /// amendment, taken as Autoplay during the change-over.
    Radio {
        on: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        by: Option<RadioBy>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        count: Option<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        mode: Option<RadioMode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        seed: Option<Vec<String>>,
        /// What to play inside (decision 0094: a crate, a playlist, a record, an artist or tracks); null plays beyond
        /// it again; left out: as it was.
        #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        within: Option<Option<Within>>,
    },
    /// Autoplay in a mode, or `stop` (decision 0036, amended): the player appends its tracks when the queue runs out.
    /// What is left out stays as it was; `within` null plays beyond what it stayed inside again. On turns repeat all off.
    Autoplay {
        mode: AutoplayMode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        albums: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        restart: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        seed: Option<Vec<String>>,
        #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        within: Option<Option<Within>>,
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
        #[cfg_attr(test, ts(optional))]
        index: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        entry: Option<String>,
    },
    /// To entry `entry` (or the one at place `index`), from its start.
    Jump {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        index: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        entry: Option<String>,
    },
    Pause,
    Resume,
    Toggle,
    /// Its queue emptied. A handover's stop names the player the music went to, `moved_to` (the owner, 2026-10-30: "the
    /// screen follows the music"): the player and every screen driving it switch to that one. Every other stop names
    /// none.
    Stop {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        moved_to: Option<String>,
        /// Back after its music was handed away while it was out of reach: stopped, its queue kept to go on with later (the
        /// owner, 2026-11-08: "Stop it, keep its queue"; the tablet, offline in a group it led, came back to an empty queue).
        /// Every other stop empties it.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        #[cfg_attr(test, ts(as = "Option<bool>", optional))]
        kept: bool,
    },
    Next,
    Previous,
    /// Previous or Next held (decision 0074): `start` pauses with the device's Start and stop, then winds `back` or
    /// `forward` (its own envelope, the server's wind stream) while held; `stop` brakes and plays on from there.
    Wind {
        dir: String,
        phase: String,
    },
    /// The tape deck's direction knob (0.9.12): `L` plays backwards at 1× on the wind stream, `R` forwards; turned while
    /// playing, the tape slows to a stop first. The player keeps it and reports it.
    Direction {
        dir: String,
    },
    /// Level the volume (0.8/0.9.8), set from a page driving the player (0.9.12): on or off, and how a track not
    /// measured is taken (`typical`, `untouched`). The player eases into its new level over a second.
    Level {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        on: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        unknown: Option<String>,
    },
    /// To `position_ms`, or `percent` (0 to 100) of the track, or `by_ms` on from where it is now (back below 0: the
    /// visuals' double and triple taps). What reaches the player is always `position_ms`.
    Seek {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        position_ms: Option<u64>,
        #[serde(default, skip_serializing)]
        percent: Option<f64>,
        #[serde(default, skip_serializing)]
        by_ms: Option<i64>,
        /// Wound there (decision 0074) whatever the player's own setting, where it plays and it is more than a moment
        /// away (0.9.12: a page driving it with its seek winding on).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        wind: bool,
    },
    /// 0 to 100.
    Volume { level: u8 },
    /// Quieter, for an announcement; `Unduck` brings it back.
    Duck,
    Unduck,
    /// Speak this (Home Assistant's `tts.speak … announce: true`, the 0.9 announce channel): the clip at `url` played over
    /// the music, which ducks by the clip's own loudness as it plays, word by word, and comes back after its last. A
    /// group's is played by its leader, mixed into the group's sound, so every member hears it in step. `id` (the
    /// caller's, else Cratarr's) names it in the player's reports (`announce`): playing, then done or failed.
    Announce {
        url: String,
        #[serde(default)]
        id: String,
    },
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
        #[cfg_attr(test, ts(optional))]
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
        #[cfg_attr(test, ts(optional))]
        host: Option<String>,
        port: u16,
        buffer_ms: u32,
        /// Its own secret, which the sync server asks of it (decision 0051): as a listener, its snapclient's id is
        /// `secret:name`; as a leader, its source hello carries `secret`.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        secret: String,
    },
    /// To an app: one short line it shows as a notification while it is not in front (a queue request asked for or
    /// answered, the owner 2026-10-30). Never taken from outside.
    Notice { text: String },
}

impl Command {
    /// Play these tracks, as a player is told to.
    pub fn play(tracks: Vec<QueueTrack>, index: usize, position_ms: u64) -> Self {
        Command::Play { from: PlayFrom::default(), tracks: Some(tracks), index, position_ms, shuffle: false, paused: false, queue_id: None, queue_name: None }
    }
}

/// Cratarr's own ids of the playing track (decision 0030): what Home Assistant's library calls take.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct TrackIds {
    pub track_id: Option<String>,
    pub album_id: Option<String>,
    pub album_artist: Option<String>,
}

/// Autoplay as the player view shows it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AutoplayView {
    pub mode: AutoMode,
    pub albums: bool,
    pub restart: bool,
}

/// Where a player's music went at a handover, and when (the owner, 2026-10-30: "the screen follows the music").
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct MovedTo {
    pub player: String,
    /// When (epoch ms).
    pub at: u64,
}

/// A player as Home Assistant and the pages see it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, rename = "PlayOnPlayer"))]
pub struct PlayerView {
    pub name: String,
    /// Connected now and heard from lately: commands reach it.
    pub online: bool,
    /// Its link, as Cratarr knows it and as the player last told (decision 0045).
    pub link: LinkView,
    /// `playing`, `paused` or `stopped` (nothing loaded).
    #[cfg_attr(test, ts(type = "\"playing\" | \"paused\" | \"stopped\""))]
    pub state: &'static str,
    /// When it was last heard from (epoch seconds).
    pub seen: u64,
    pub playing: bool,
    pub index: i64,
    /// Now: counted on from the last report while it plays; while it winds, at the speed it says (0.11.1).
    pub position_ms: u64,
    /// When `position_ms` was so (epoch ms): from then on it moves with the music while `playing`, or at `speed_x`.
    pub position_at: u64,
    pub duration_ms: Option<u64>,
    pub volume: u8,
    pub ducked: bool,
    /// The announcement it plays or last played, as its app says; in a group, the leader's (the group hears it).
    pub announce: Option<Announcing>,
    /// Why it does not play what it was told to, as it says (its device would not let it); none: nothing holds it.
    pub held: Option<String>,
    /// Tape rewind under way (0.9.10): `position_ms` goes back to the start, reported as it goes, not counted on.
    pub rewinding: bool,
    /// Asked to play from rest and not sounding yet, as the player says (0.9.14).
    pub starting: bool,
    /// How it moves, as the player says (0075; 0.11.1: a driven player's wind shown as it is, 2026-10-30): track ms per
    /// ms, signed, and how fast that changes a second. Left out where it does not say (a page says it only while it
    /// winds or holds): then 1 playing, 0 paused.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub speed_x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub accel_x: Option<f64>,
    /// Where its hold lands, as the player says (0.11.1): null once it holds no more; left out where it does not say.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub landing: Option<Option<Landing>>,
    /// The direction knob as the player says (0.9.12): `L` or `R`.
    #[cfg_attr(test, ts(type = "\"L\" | \"R\""))]
    pub direction: &'static str,
    /// Level the volume as the player says (0.9.12): on, how an unmeasured track is taken, what it turns down by now.
    pub levelling: bool,
    pub level_unknown: Option<String>,
    pub level_db: Option<f64>,
    /// Where its sound goes now, as its app says (decision 0033, amended 2026-10-19); none where it does not say.
    pub output: Option<Output>,
    pub queue_length: usize,
    /// Off, one or all (decision 0033, amended).
    pub repeat: Repeat,
    /// Always false: there is no shuffle state (the owner, 2026-10-23: shuffle is once, `shuffle`). Kept for Home
    /// Assistant, whose shuffle switch reads it.
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
    /// It has been in a group (`PlayOnSettings::grouped`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(as = "Option<bool>", optional))]
    pub grouped: bool,
    /// Shared with everyone on this Cratarr (0.9.8).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(as = "Option<bool>", optional))]
    pub shared: bool,
    /// The asking account's own (its settings theirs to change); a shared one of another's is not. Its owner's name
    /// beside it when it is not.
    pub mine: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub owner: Option<String>,
    /// `app` (the Android app) or `kodi` (a Kodi Cratarr drives): what it can do (a Kodi joins no group).
    #[cfg_attr(test, ts(type = "\"app\" | \"kodi\" | \"browser\""))]
    pub kind: &'static str,
    /// The whole queue, for one player asked for by name.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub queue: Option<Vec<QueueTrack>>,
    /// Its queues (decision 0033, amended: queue history), the one that plays marked, newest first: in every view, so
    /// what is pushed (the players stream, Home Assistant's webhook) carries them.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub queues: Option<Vec<QueueItem>>,
    /// Goes up whenever that list changes: ask for the player by name to see it.
    pub queues_version: u64,
    /// `cratarr`, or `device` while it plays the device's own music (decision 0052): then `title`, `artist`, `album`,
    /// `state` and `position_ms` are that music's, and nothing of Cratarr's queue plays (`track` none, `index` -1,
    /// `queue_length` 0; the queue itself waits, as do its queues).
    #[cfg_attr(test, ts(type = "\"cratarr\" | \"device\""))]
    pub mode: &'static str,
    /// The player it follows, by its user and name (decision 0079: listening along): what it plays is kept in line
    /// with that player's.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub following: Option<FollowTarget>,
    /// The users following it (decision 0079), by name.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(test, ts(as = "Option<Vec<String>>", optional))]
    pub followers: Vec<String>,
    /// Queue requests waiting for its owner's answer (decision 0079); on the asking account's own players only.
    #[serde(skip_serializing_if = "is_zero")]
    #[cfg_attr(test, ts(as = "Option<usize>", optional))]
    pub queue_requests: usize,
    /// Where its music went, as a handover moved it (the owner, 2026-10-30: "the screen follows the music"): a screen
    /// showing this player switches to that one. Until it plays again, five minutes at most; only to one who may reach
    /// that player.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub moved_to: Option<MovedTo>,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// A queue in a player's list, as a page shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
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
