//! A player's queue as Cratarr holds it (decision 0033): entries with ids of their own, edited by entry, with a version
//! that goes up on every change. Pure: Play On keeps one per player, saves it and tells the player.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use cratarr_rules::{ByHand, Lane};

use super::ToggleError;
use super::playon::{Command, QueueTrack};
use super::radio::{Autoplay, AutoplayMode, LANE_AUTOPLAY, LANE_RADIO, RADIO_COUNT, RADIO_MAX, RadioBy, from_old};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Queue {
    pub version: u64,
    pub tracks: Vec<QueueTrack>,
    /// Repeat (decision 0033, amended): a setting of the queue; the player that plays applies it when a track ends.
    #[serde(default)]
    pub repeat: Repeat,
    /// Radio (decision 0036, amended): while on, about five tracks like each of the queue's own tracks are inserted
    /// right after it as it starts. The player that plays inserts them. (Saved as `radio_on`: `radio` was Autoplay's
    /// object before the amendment.)
    #[serde(default, rename = "radio_on", skip_serializing_if = "std::ops::Not::not")]
    pub radio: bool,
    /// What Radio's inserts are like, on or off.
    #[serde(default, skip_serializing_if = "RadioBy::is_track")]
    pub radio_by: RadioBy,
    /// How many follow each own track (unset: `RADIO_COUNT`), on or off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radio_count: Option<u8>,
    /// Radio and Autoplay leave the owner's favourites out (the More music sheet's "Ignore favourites").
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip_favourites: bool,
    /// Autoplay: on while there is one; the player that plays appends its tracks when the queue runs out. Excludes
    /// repeat all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autoplay: Option<Autoplay>,
    /// When something new started in it (epoch seconds; 0: before this was kept): a play, a handover to this player,
    /// or a queue a player reports that keeps none of the album's or playlist's entries. Edits, Radio's and
    /// Autoplay's inserts and the music playing on leave it, so it says which queue was started last (the Subsonic
    /// saved queue, decision 0033 step 5).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub started: u64,
    /// Which queue this is (decision 0033, amended: queue history): a new one with each new play, kept with it when
    /// it is left behind; empty for a queue from before.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// Its name: the play's own (a playlist's, an album's title), until the owner renames it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Renamed by the owner: kept until deleted, however many come after it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub renamed: bool,
    /// What it was started from: `playlist`, `album`, `favourites`, `library`, `tracks`, or `player` (started there).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Where it was heard (decision 0033, amended 2026-10-19: the players' audio outputs), oldest first, at most
    /// `PLAYED_ON` different ones: it moves with the queue (a handover, a switch).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub played_on: Vec<PlayedOn>,
}

/// Where a queue was heard: on which player, through what (its app's audio output: `bluetooth`, `speaker`, `wired`,
/// `usb`, `hdmi`, `remote`, `other`; none where the player does not say) and the device's name where it has one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PlayedOn {
    pub player: String,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

/// How many places a queue remembers it was heard.
const PLAYED_ON: usize = 4;

impl PlayedOn {
    /// Whether `text` names it: in its device's name or its player's, whatever the case.
    pub fn named(&self, text: &str) -> bool {
        let t = text.to_lowercase();
        self.name.as_deref().is_some_and(|n| n.to_lowercase().contains(&t)) || self.player.to_lowercase().contains(&t)
    }
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The entries of the album or playlist that plays (not those added by hand, nor Radio's or Autoplay's).
fn own_entries(tracks: &[QueueTrack]) -> impl Iterator<Item = &str> {
    tracks.iter().filter(|t| t.lane.is_none() && !t.entry.is_empty()).map(|t| t.entry.as_str())
}

/// Off: the queue ends. One: the playing track starts again. All: after the last track the album or playlist starts
/// again (what was added by hand plays once).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Repeat {
    #[default]
    Off,
    One,
    All,
}

/// An entry id no queue has had before: a version 4 UUID.
pub fn new_entry() -> String {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let n = COUNT.fetch_add(1, Ordering::Relaxed);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    // Two hashers with the process's random keys over a counter and the clock: unpredictable, and never twice.
    let half = |salt: u64| {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(salt);
        h.write_u64(n);
        h.write_u128(now);
        h.finish()
    };
    let (a, b) = (half(1), half(2));
    let a = (a & 0xffff_ffff_ffff_0fff) | 0x4000;
    let b = (b & 0x3fff_ffff_ffff_ffff) | 0x8000_0000_0000_0000;
    format!("{:08x}-{:04x}-{:04x}-{:04x}-{:012x}", a >> 32, (a >> 16) & 0xffff, a & 0xffff, b >> 48, b & 0xffff_ffff_ffff)
}

/// Tracks given entries where they came without one.
fn with_entries(tracks: Vec<QueueTrack>) -> Vec<QueueTrack> {
    tracks
        .into_iter()
        .map(|mut t| {
            if t.entry.is_empty() {
                t.entry = new_entry();
            }
            t
        })
        .collect()
}

/// The lane of tracks added by hand (`QueueTrack::lane`).
const LANE_NEXT: &str = "next";

/// Each track's lane, as the rules take them (cratarr-rules).
fn lanes(tracks: &[QueueTrack]) -> Vec<Lane> {
    tracks.iter().map(|t| Lane::of(t.lane.as_deref())).collect()
}

fn missing() -> ToggleError {
    ToggleError::Refused("That entry is not in the queue.".into())
}

fn out_of_range() -> ToggleError {
    ToggleError::Refused("There is no track at that place in the queue.".into())
}

impl Queue {
    /// The place of an entry.
    pub fn at(&self, entry: &str) -> Option<usize> {
        self.tracks.iter().position(|t| t.entry == entry)
    }

    /// The place of the entry named, or the place given.
    fn place(&self, entry: Option<&str>, index: Option<usize>) -> Result<usize, ToggleError> {
        match (entry, index) {
            (Some(e), _) => self.at(e).ok_or_else(missing),
            (None, Some(i)) if i < self.tracks.len() => Ok(i),
            (None, Some(_)) => Err(out_of_range()),
            (None, None) => Err(ToggleError::Refused("Which entry? Give its id or its place.".into())),
        }
    }

    /// The entry before place `p`, as an anchor: "" for the first place.
    fn anchor(&self, p: usize) -> String {
        p.checked_sub(1).and_then(|i| self.tracks.get(i)).map(|t| t.entry.clone()).unwrap_or_default()
    }

    /// The tracks added by hand still to come: the run of them right after the playing one, `current` (0033's lanes).
    fn added_ahead(&self, current: Option<usize>) -> Vec<QueueTrack> {
        let Some(c) = current else { return Vec::new() };
        let n = cratarr_rules::added_ahead(&lanes(&self.tracks), current);
        self.tracks.iter().skip(c + 1).take(n).cloned().collect()
    }

    /// A lane's tracks still to come, out (Radio or Autoplay going off, repeat all coming on): `true` when there were any.
    /// How many tracks Radio inserts after each own track.
    pub fn radio_count(&self) -> u8 {
        self.radio_count.unwrap_or(RADIO_COUNT)
    }

    fn drop_lane_ahead(&mut self, current: Option<usize>, lane: &str) -> bool {
        let keep = current.map_or(0, |c| c + 1).min(self.tracks.len());
        let before = self.tracks.len();
        let mut i = 0;
        self.tracks.retain(|t| {
            i += 1;
            i <= keep || t.lane.as_deref() != Some(lane)
        });
        self.tracks.len() != before
    }

    /// Autoplay on in a mode (what is not given stays as it was; `stop` switches it off, its tracks to come out). On
    /// turns repeat all off: both say what comes when the queue ends. Returns the command as the player is told it.
    fn set_autoplay(
        &mut self,
        mode: AutoplayMode,
        albums: Option<bool>,
        restart: Option<bool>,
        seed: Option<Vec<String>>,
        within: Option<String>,
        current: Option<usize>,
    ) -> Command {
        let Some(mode) = mode.on() else {
            self.autoplay = None;
            if self.drop_lane_ahead(current, LANE_AUTOPLAY) {
                self.version += 1;
            }
            return Command::Autoplay { mode: AutoplayMode::Stop, albums: None, restart: None, seed: None, within: None };
        };
        let was_on = self.autoplay.is_some();
        let was = self.autoplay.take().unwrap_or_default();
        let within = match within {
            Some(w) if w.is_empty() => None,
            Some(w) => Some(w),
            None => was.within.clone(),
        };
        let a = Autoplay {
            mode,
            albums: albums.unwrap_or(was.albums),
            restart: restart.unwrap_or(was.restart),
            seed: seed.unwrap_or(was.seed),
            within,
        };
        // Another mode, albums or crate: what it chose before makes way for what it chooses now.
        if was_on && (was.mode != a.mode || was.albums != a.albums || was.within != a.within) && self.drop_lane_ahead(current, LANE_AUTOPLAY) {
            self.version += 1;
        }
        self.autoplay = Some(a.clone());
        if self.repeat == Repeat::All {
            self.repeat = Repeat::Off;
        }
        Command::Autoplay {
            mode: AutoplayMode::of(Some(a.mode)),
            albums: Some(a.albums),
            restart: Some(a.restart),
            seed: Some(a.seed),
            within: Some(a.within.unwrap_or_default()),
        }
    }

    /// `edit_at` with no track playing.
    #[cfg(test)]
    pub fn edit(&mut self, command: Command) -> Result<Command, ToggleError> {
        self.edit_at(command, None)
    }

    /// Makes an edit, and returns it as the player is told: by entry, and by place too, for apps before decision 0033.
    /// Commands that do not touch the queue come back as they were. `current`: the place of the track that plays.
    pub fn edit_at(&mut self, command: Command, current: Option<usize>) -> Result<Command, ToggleError> {
        let told = match command {
            // Something new replaces the album or playlist, not what was added by hand: that comes right after its first
            // track (0033's lanes). What the new queue holds already (a queue moved whole) is not added twice.
            Command::Play { from, tracks: Some(tracks), index, position_ms, shuffle, paused, queue_id, queue_name } => {
                let mut tracks = with_entries(tracks);
                // A queue taken up whole (one switched back to) brings nothing of the one it leaves: that is kept as it
                // stood, its own tracks with it.
                if !from.whole {
                    let ahead: Vec<QueueTrack> =
                        self.added_ahead(current).into_iter().filter(|t| !tracks.iter().any(|x| x.entry == t.entry)).collect();
                    let at = (index + 1).min(tracks.len());
                    tracks.splice(at..at, ahead);
                }
                self.tracks = tracks;
                self.started = now_secs();
                // Another queue (decision 0033, amended): its own id and name, not renamed until it is, heard nowhere yet.
                if let Some(id) = &queue_id {
                    self.id = id.clone();
                    self.name = queue_name.clone().unwrap_or_default();
                    self.renamed = false;
                    self.played_on.clear();
                }
                // Something new turns repeat one off; repeat all stays (decision 0033, amended, rule 4).
                if self.repeat == Repeat::One {
                    self.repeat = Repeat::Off;
                }
                // Radio and Autoplay stay on (decision 0036, amended); Autoplay now after the new album or playlist, which
                // is its seed. Their old tracks went with the rest of the old queue. A play of Autoplay's tracks only is
                // Autoplay starting from an item ("Start radio"): its seed came, or comes, with its own command.
                if let Some(a) = &mut self.autoplay
                    && !self.tracks.iter().all(|t| t.lane.as_deref() == Some(LANE_AUTOPLAY))
                {
                    a.seed.clear();
                    a.within = None;
                }
                Command::Play { from, tracks: Some(self.tracks.clone()), index, position_ms, shuffle, paused, queue_id, queue_name }
            }
            Command::Add { tracks, at, after, how, .. } => {
                let mut tracks = with_entries(tracks);
                let end = self.tracks.len();
                // Added by hand: right after the playing track (`next`, the newest first), or after those added before
                // (`last`), ahead of the album or playlist either way (the rules' own, cratarr-rules).
                let by_hand = match how.as_deref() {
                    Some("next") => Some(cratarr_rules::add_place(&lanes(&self.tracks), current, ByHand::Next)),
                    Some("last") => Some(cratarr_rules::add_place(&lanes(&self.tracks), current, ByHand::Last)),
                    _ => None,
                };
                if by_hand.is_some() {
                    tracks.iter_mut().for_each(|t| t.lane = Some(LANE_NEXT.into()));
                }
                let p = match (by_hand, after.as_deref()) {
                    (Some(p), _) => p.min(end),
                    (None, Some("")) => 0,
                    (None, Some(a)) => self.at(a).ok_or_else(missing)? + 1,
                    (None, None) => at.unwrap_or(end).min(end),
                };
                let after = self.anchor(p);
                self.tracks.splice(p..p, tracks.iter().cloned());
                Command::Add { from: Default::default(), tracks, at: Some(p), after: Some(after), how: None }
            }
            Command::Move { from, to, entry, after } => {
                let from = self.place(entry.as_deref(), from)?;
                // The place it ends up at, counted with it taken out.
                let to = match after.as_deref() {
                    Some("") => 0,
                    Some(a) => match self.at(a).ok_or_else(missing)? {
                        a if a == from => return Err(ToggleError::Refused("An entry cannot go after itself.".into())),
                        a if a > from => a,
                        a => a + 1,
                    },
                    None => to.ok_or_else(|| ToggleError::Refused("Move it where?".into()))?.min(self.tracks.len() - 1),
                };
                let moved = self.tracks.remove(from);
                let entry = moved.entry.clone();
                self.tracks.insert(to, moved);
                Command::Move { from: Some(from), to: Some(to), entry: Some(entry), after: Some(self.anchor(to)) }
            }
            // Everything after the playing track, replaced; the playing track plays on.
            Command::Upcoming { tracks, shuffled } => {
                let keep = current.map_or(0, |c| c + 1).min(self.tracks.len());
                let tracks = with_entries(tracks);
                self.tracks.truncate(keep);
                self.tracks.extend(tracks.iter().cloned());
                Command::Upcoming { tracks, shuffled }
            }
            // Shuffle, once (the owner, 2026-10-23: a queue always has an order): the rules' one-time shuffle
            // (cratarr-rules); what was added by hand, Radio's inserts and Autoplay's tail stay where they are. Off has no
            // state to turn off: the order stays. Told as the upcoming tracks, never shuffled (a player from before keeps a
            // switch of its own for repeat all's rounds: it goes off).
            Command::Shuffle { on } => {
                let c = current.filter(|c| *c < self.tracks.len()).ok_or_else(|| ToggleError::Refused("Nothing plays.".into()))?;
                if on && let Some(order) = cratarr_rules::shuffle_order(&lanes(&self.tracks), Some(c), &mut super::play::rng()) {
                    let upcoming = cratarr_rules::take(&self.tracks, &order);
                    self.tracks.truncate(c + 1);
                    self.tracks.extend(upcoming);
                }
                Command::Upcoming { tracks: self.tracks[c + 1..].to_vec(), shuffled: Some(false) }
            }
            // Everything after the playing track, out (the playing track plays on); all of it when nothing plays.
            Command::Clear => {
                self.tracks.truncate(cratarr_rules::upcoming_start(current, self.tracks.len()));
                Command::Upcoming { tracks: Vec::new(), shuffled: None }
            }
            // Undo of a shuffle or a clear (cratarr-rules): what was to come, less what played since, in place of what is
            // to come now.
            Command::Restore { tracks } => {
                let start = cratarr_rules::upcoming_start(current, self.tracks.len());
                let entries: Vec<&str> = self.tracks.iter().map(|t| t.entry.as_str()).collect();
                let before: Vec<&str> = tracks.iter().map(|t| t.entry.as_str()).collect();
                let back = with_entries(cratarr_rules::take(&tracks, &cratarr_rules::restore_order(&entries, current, &before)));
                self.tracks.truncate(start);
                self.tracks.extend(back.iter().cloned());
                Command::Upcoming { tracks: back, shuffled: Some(false) }
            }
            Command::Remove { index, entry } => {
                let i = self.place(entry.as_deref(), index)?;
                let gone = self.tracks.remove(i);
                Command::Remove { index: Some(i), entry: Some(gone.entry) }
            }
            // A setting, not a change of the tracks: the queue's version stays. Repeat all turns Autoplay off (decision
            // 0036), and its tracks to come go: that is an edit. Radio lives with both.
            Command::Repeat { mode } => {
                self.repeat = mode;
                if mode == Repeat::All && self.autoplay.take().is_some() && self.drop_lane_ahead(current, LANE_AUTOPLAY) {
                    self.version += 1;
                }
                return Ok(Command::Repeat { mode });
            }
            // Radio from before the amendment (with a mode, seed or bound) is Autoplay now: taken as such during the
            // change-over (decision 0036, amended).
            Command::Radio { on, mode, seed, within, .. } if mode.is_some() || seed.is_some() || within.is_some() => {
                let (auto, albums, restart) = from_old(mode);
                let mode = if on { AutoplayMode::of(Some(auto)) } else { AutoplayMode::Stop };
                return Ok(self.set_autoplay(mode, Some(albums), Some(restart), seed, within, current));
            }
            // Radio on: the player inserts as each own track starts. Off: its inserts still to come go; so do they when
            // what they are like, or how many, changes while on (the player inserts anew for the playing track). What
            // is left out stays. Autoplay is not touched: the player's radio switch must not stop it (Home Assistant
            // moves to `autoplay` itself). Told in full.
            Command::Radio { on, by, count, .. } => {
                let was = (self.radio, self.radio_by, self.radio_count());
                if let Some(b) = by {
                    self.radio_by = b;
                }
                if let Some(n) = count {
                    self.radio_count = Some(n.clamp(1, RADIO_MAX));
                }
                self.radio = on;
                let other = (was.1, was.2) != (self.radio_by, self.radio_count());
                if (!on || (was.0 && other)) && self.drop_lane_ahead(current, LANE_RADIO) {
                    self.version += 1;
                }
                return Ok(Command::Radio { on, mode: None, seed: None, within: None, by: Some(self.radio_by), count: Some(self.radio_count()) });
            }
            // Favourites in or out of Radio's and Autoplay's picks: a change takes their picks still to come out, and the
            // player picks anew (Radio for the playing track, Autoplay when the queue runs low).
            Command::SkipFavourites { on } => {
                if on != self.skip_favourites {
                    self.skip_favourites = on;
                    let radio = self.drop_lane_ahead(current, LANE_RADIO);
                    let auto = self.drop_lane_ahead(current, LANE_AUTOPLAY);
                    if radio || auto {
                        self.version += 1;
                    }
                }
                return Ok(Command::SkipFavourites { on });
            }
            Command::Autoplay { mode, albums, restart, seed, within } => {
                return Ok(self.set_autoplay(mode, albums, restart, seed, within, current));
            }
            // Not an edit: the entry that plays is the player's to say.
            Command::Jump { index, entry } => {
                let i = self.place(entry.as_deref(), index)?;
                return Ok(Command::Jump { index: Some(i), entry: Some(self.tracks[i].entry.clone()) });
            }
            Command::Stop { moved_to } => {
                self.tracks.clear();
                Command::Stop { moved_to }
            }
            other => return Ok(other),
        };
        self.version += 1;
        Ok(told)
    }

    /// It is heard here: the newest of where it was heard (again, it moves last); `true` when that changed.
    pub fn heard(&mut self, on: PlayedOn) -> bool {
        if self.played_on.last() == Some(&on) {
            return false;
        }
        self.played_on.retain(|p| *p != on);
        self.played_on.push(on);
        if self.played_on.len() > PLAYED_ON {
            self.played_on.remove(0);
        }
        true
    }

    /// [`Queue::adopted`]: `true` when the queue changed.
    #[cfg(test)]
    pub fn adopt(&mut self, reported: Vec<QueueTrack>) -> bool {
        self.adopted(reported).is_some()
    }

    /// The queue a player reports, taken as it is: the player holds what plays (decision 0033, step 1). Tracks it
    /// sends without entries (an app from before 0033) keep the entries they had here, in order, so an entry keeps
    /// its id through moves; who asked for an entry stays with it where an app from before 0.10 leaves it out. None
    /// when nothing changed; else whether it is another queue: something new the player started itself, or none at all
    /// (the one before is left behind, decision 0033 amended).
    pub fn adopted(&mut self, reported: Vec<QueueTrack>) -> Option<bool> {
        let mut known: Vec<Option<&QueueTrack>> = self.tracks.iter().map(Some).collect();
        let asked: std::collections::HashMap<&str, &str> =
            self.tracks.iter().filter(|t| !t.entry.is_empty()).filter_map(|t| Some((t.entry.as_str(), t.asked_by.as_deref()?))).collect();
        let tracks: Vec<QueueTrack> = reported
            .into_iter()
            .map(|mut t| {
                if t.entry.is_empty() {
                    let same = known.iter_mut().find(|k| k.is_some_and(|k| k.source == t.source && k.id == t.id));
                    t.entry = same.and_then(Option::take).map_or_else(new_entry, |k| k.entry.clone());
                }
                if t.asked_by.is_none()
                    && let Some(who) = asked.get(t.entry.as_str())
                {
                    t.asked_by = Some((*who).to_string());
                }
                t
            })
            .collect();
        if tracks == self.tracks {
            return None;
        }
        // Something new the player started itself: none of the album's or playlist's entries are left (a queue of
        // tracks added by hand alone: none of its entries).
        let fresh = if own_entries(&tracks).next().is_some() {
            let own: std::collections::HashSet<&str> = own_entries(&self.tracks).collect();
            !own_entries(&tracks).any(|e| own.contains(e))
        } else {
            !tracks.iter().any(|t| self.tracks.iter().any(|o| o.entry == t.entry))
        };
        if fresh && !tracks.is_empty() {
            self.started = now_secs();
        }
        let other = fresh || tracks.is_empty();
        self.tracks = tracks;
        self.version += 1;
        Some(other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: &str) -> QueueTrack {
        QueueTrack { source: "cratarr".into(), id: id.into(), name: id.into(), ..QueueTrack::default() }
    }

    fn ids(q: &Queue) -> String {
        q.tracks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>().join(" ")
    }

    fn played(ids: &[&str]) -> Queue {
        let mut q = Queue::default();
        q.edit(Command::play(ids.iter().map(|i| track(i)).collect(), 0, 0)).unwrap();
        q
    }

    #[test]
    fn entries_are_new_each_time_and_look_like_uuids() {
        let (a, b) = (new_entry(), new_entry());
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4", "version 4");
        let mut q = played(&["a", "a"]);
        assert_ne!(q.tracks[0].entry, q.tracks[1].entry, "the same track twice: two entries");
        assert_eq!(q.version, 1);
        q.edit(Command::Pause).unwrap();
        assert_eq!(q.version, 1, "not an edit");
    }

    #[test]
    fn edits_by_entry_land_where_the_entry_is_now() {
        let mut q = played(&["a", "b", "c", "d"]);
        let (a, c, d) = (q.tracks[0].entry.clone(), q.tracks[2].entry.clone(), q.tracks[3].entry.clone());
        // Moved by entry, after another: told by both, the place counted with it taken out.
        let told = q.edit(Command::Move { from: None, to: None, entry: Some(a.clone()), after: Some(c.clone()) }).unwrap();
        assert_eq!(ids(&q), "b c a d");
        assert_eq!(told, Command::Move { from: Some(0), to: Some(2), entry: Some(a.clone()), after: Some(c.clone()) });
        let told = q.edit(Command::Move { from: None, to: None, entry: Some(d.clone()), after: Some(String::new()) }).unwrap();
        assert_eq!(ids(&q), "d b c a");
        assert_eq!(told, Command::Move { from: Some(3), to: Some(0), entry: Some(d.clone()), after: Some(String::new()) });
        // By place, as Home Assistant sends it: told by entry too.
        let told = q.edit(Command::Move { from: Some(0), to: Some(3), entry: None, after: None }).unwrap();
        assert_eq!(ids(&q), "b c a d");
        assert_eq!(told, Command::Move { from: Some(0), to: Some(3), entry: Some(d.clone()), after: Some(a.clone()) });
        // Removed by entry wherever it went.
        let told = q.edit(Command::Remove { index: None, entry: Some(c.clone()) }).unwrap();
        assert_eq!(ids(&q), "b a d");
        assert_eq!(told, Command::Remove { index: Some(1), entry: Some(c.clone()) });
        assert!(q.edit(Command::Remove { index: None, entry: Some(c) }).is_err(), "gone already");
        assert!(q.edit(Command::Move { from: None, to: None, entry: Some(a.clone()), after: Some(a) }).is_err());
        assert!(q.edit(Command::Remove { index: Some(9), entry: None }).is_err());
        assert_eq!(q.version, 5);
    }

    #[test]
    fn added_tracks_go_after_their_anchor_together() {
        let mut q = played(&["a", "b"]);
        let a = q.tracks[0].entry.clone();
        let told = q.edit(Command::Add { from: Default::default(), tracks: vec![track("x"), track("y")], at: None, after: Some(a.clone()), how: None }).unwrap();
        assert_eq!(ids(&q), "a x y b");
        let Command::Add { tracks, at, after, .. } = told else { panic!() };
        assert_eq!((at, after), (Some(1), Some(a)));
        assert!(tracks.iter().all(|t| !t.entry.is_empty()), "the player is told their entries");
        q.edit(Command::Add { from: Default::default(), tracks: vec![track("z")], at: None, after: None, how: None }).unwrap();
        assert_eq!(ids(&q), "a x y b z", "at the end");
        let Command::Add { after, .. } = q.edit(Command::Add { from: Default::default(), tracks: vec![track("w")], at: Some(0), after: None, how: None }).unwrap() else { panic!() };
        assert_eq!((ids(&q).as_str(), after), ("w a x y b z", Some(String::new())));
    }

    #[test]
    fn tracks_added_by_hand_stay_ahead_of_the_album_and_outlive_a_new_one() {
        let mut q = played(&["a", "b", "c"]);
        let add = |q: &mut Queue, ids: &[&str], how: &str| {
            q.edit_at(Command::Add { from: Default::default(), tracks: ids.iter().map(|i| track(i)).collect(), at: None, after: None, how: Some(how.into()) }, Some(0)).unwrap()
        };
        // Playing a: "add to queue" goes after a, ahead of b and c; the next one after it; "play next" before both.
        add(&mut q, &["x"], "last");
        add(&mut q, &["y"], "last");
        assert_eq!(ids(&q), "a x y b c");
        let Command::Add { at, .. } = add(&mut q, &["n1", "n2"], "next") else { panic!() };
        assert_eq!((ids(&q).as_str(), at), ("a n1 n2 x y b c", Some(1)), "the newest batch first, kept together");
        assert!(q.tracks[1..5].iter().all(|t| t.lane.as_deref() == Some("next")) && q.tracks[5].lane.is_none());
        // Something new played: the album is replaced, what was added by hand comes right after its first track.
        q.edit_at(Command::play(vec![track("p"), track("q")], 0, 0), Some(0)).unwrap();
        assert_eq!(ids(&q), "p n1 n2 x y q");
        // A queue moved here whole, holding them already: not twice.
        let whole = q.tracks.clone();
        q.edit_at(Command::play(whole, 0, 0), Some(0)).unwrap();
        assert_eq!(ids(&q), "p n1 n2 x y q");
        // Nothing playing: added at the end.
        let mut empty = Queue::default();
        empty.edit_at(Command::Add { from: Default::default(), tracks: vec![track("z")], at: None, after: None, how: Some("last".into()) }, None).unwrap();
        assert_eq!(ids(&empty), "z");
    }

    #[test]
    fn a_shuffle_is_once_and_its_undo_puts_the_order_before_back() {
        let tracks: Vec<QueueTrack> = (0..12u32).map(|i| QueueTrack { n: Some(i), ..track(&format!("t{i}")) }).collect();
        let mut q = Queue::default();
        q.edit(Command::play(tracks, 0, 0)).unwrap();
        // Playing t0, one added by hand ahead.
        q.edit_at(Command::Add { from: Default::default(), tracks: vec![track("x")], at: None, after: None, how: Some("next".into()) }, Some(0)).unwrap();
        let before = q.tracks[1..].to_vec();
        let Command::Upcoming { tracks: told, shuffled } = q.edit_at(Command::Shuffle { on: true }, Some(0)).unwrap() else { panic!() };
        assert_eq!(shuffled, Some(false), "no shuffle state: a player from before turns its own off");
        assert_eq!(told, q.tracks[1..], "told the upcoming tracks as they are now");
        assert_eq!(told.len(), 12, "every track still there");
        assert_eq!(told[0].id, "x", "what was added by hand stays ahead");
        let mut ns: Vec<u32> = told[1..].iter().filter_map(|t| t.n).collect();
        assert_ne!(ns, (1..12).collect::<Vec<_>>(), "11 tracks in their own order after a shuffle: one chance in 40 million");
        ns.sort();
        assert_eq!(ns, (1..12).collect::<Vec<_>>());
        // Off has nothing to turn off: the order stays.
        let now = q.tracks.clone();
        q.edit_at(Command::Shuffle { on: false }, Some(0)).unwrap();
        assert_eq!(q.tracks, now);
        // On to the third; Undo: the order before, less the two played since, after the playing one.
        let played: Vec<String> = q.tracks[1..=2].iter().map(|t| t.entry.clone()).collect();
        let Command::Upcoming { tracks: told, .. } = q.edit_at(Command::Restore { tracks: before.clone() }, Some(2)).unwrap() else { panic!() };
        let want: Vec<QueueTrack> = before.into_iter().filter(|t| !played.contains(&t.entry)).collect();
        assert_eq!(told, want);
        assert_eq!(q.tracks[3..], want[..], "in the order before, by the same entries");
        assert!(q.edit_at(Command::Shuffle { on: true }, None).is_err(), "nothing plays");
        // `shuffle` alone is once, as `on: true`.
        assert_eq!(serde_json::from_str::<Command>(r#"{"command":"shuffle"}"#).unwrap(), Command::Shuffle { on: true });
    }

    #[test]
    fn who_asked_stays_with_its_entry_through_a_shuffle_its_undo_and_a_report() {
        let mut q = played(&["a", "b", "c", "d", "e"]);
        let asked = QueueTrack { asked_by: Some("Guest Sam".into()), ..track("x") };
        q.edit_at(Command::Add { from: Default::default(), tracks: vec![asked], at: None, after: None, how: Some("last".into()) }, Some(0)).unwrap();
        let x = |q: &Queue| q.tracks.iter().find(|t| t.id == "x").map(|t| (t.entry.clone(), t.asked_by.clone()));
        let (entry, who) = x(&q).unwrap();
        assert_eq!(who.as_deref(), Some("Guest Sam"));
        let before = q.tracks[1..].to_vec();
        q.edit_at(Command::Shuffle { on: true }, Some(0)).unwrap();
        assert_eq!(x(&q), Some((entry.clone(), who.clone())));
        q.edit_at(Command::Restore { tracks: before }, Some(0)).unwrap();
        assert_eq!(x(&q), Some((entry.clone(), who.clone())));
        let upcoming = q.tracks[1..].iter().rev().cloned().collect();
        q.edit_at(Command::Upcoming { tracks: upcoming, shuffled: None }, Some(0)).unwrap();
        assert_eq!(x(&q), Some((entry.clone(), who.clone())));
        // An app from before it reports the queue without it, by entry or with none: the entry keeps it.
        let reported: Vec<QueueTrack> = q.tracks.iter().map(|t| QueueTrack { asked_by: None, ..t.clone() }).collect();
        assert!(!q.adopt(reported.clone()), "nothing changed");
        assert_eq!(x(&q), Some((entry.clone(), who.clone())));
        let unnamed: Vec<QueueTrack> = reported.into_iter().rev().map(|t| QueueTrack { entry: String::new(), ..t }).collect();
        assert!(q.adopt(unnamed));
        assert_eq!(x(&q), Some((entry, who)));
        assert_eq!(q.tracks.iter().filter(|t| t.asked_by.is_some()).count(), 1, "none of the others");
    }

    #[test]
    fn a_clear_takes_what_is_to_come_out_and_its_undo_puts_it_back() {
        let mut q = played(&["a", "b", "c", "d"]);
        let before = q.tracks[2..].to_vec();
        let version = q.version;
        let Command::Upcoming { tracks: told, .. } = q.edit_at(Command::Clear, Some(1)).unwrap() else { panic!() };
        assert!(told.is_empty());
        assert_eq!((ids(&q), q.version), ("a b".into(), version + 1), "the playing track plays on");
        // Autoplay brought one meanwhile; Undo puts back what was cleared in its place.
        q.edit_at(Command::Add { from: Default::default(), tracks: vec![in_lane("p", LANE_AUTOPLAY)], at: None, after: None, how: None }, Some(1)).unwrap();
        q.edit_at(Command::Restore { tracks: before }, Some(1)).unwrap();
        assert_eq!(ids(&q), "a b c d");
        // Nothing plays: all of it.
        q.edit_at(Command::Clear, None).unwrap();
        assert_eq!(ids(&q), "");
        assert_eq!(serde_json::from_str::<Command>(r#"{"command":"clear"}"#).unwrap(), Command::Clear);
    }

    fn in_lane(id: &str, lane: &str) -> QueueTrack {
        QueueTrack { lane: Some(lane.into()), ..track(id) }
    }

    fn auto(mode: AutoplayMode) -> Command {
        Command::Autoplay { mode, albums: None, restart: None, seed: None, within: None }
    }

    #[test]
    fn autoplay_and_repeat_all_exclude_each_other_and_stop_takes_its_tracks_out() {
        let mut q = played(&["a", "b"]);
        q.edit(Command::Repeat { mode: Repeat::All }).unwrap();
        let v = q.version;
        let told = q.edit(auto(AutoplayMode::Similar)).unwrap();
        assert_eq!(
            told,
            Command::Autoplay { mode: AutoplayMode::Similar, albums: Some(false), restart: Some(false), seed: Some(vec![]), within: Some(String::new()) }
        );
        assert_eq!((q.repeat, q.version), (Repeat::Off, v), "on turns repeat all off; a setting, the version stays");
        // The player appended Autoplay's picks; playing b.
        q.edit_at(Command::Add { from: Default::default(), tracks: vec![in_lane("p1", LANE_AUTOPLAY), in_lane("p2", LANE_AUTOPLAY)], at: None, after: None, how: None }, Some(1)).unwrap();
        let v = q.version;
        q.edit_at(Command::Autoplay { mode: AutoplayMode::Similar, albums: None, restart: Some(true), seed: None, within: None }, Some(1)).unwrap();
        assert_eq!((ids(&q), q.version), ("a b p1 p2".to_string(), v), "restart alone keeps the picks");
        q.edit_at(auto(AutoplayMode::Era), Some(1)).unwrap();
        assert_eq!((ids(&q), q.version), ("a b".to_string(), v + 1), "another mode: its old picks make way");
        assert_eq!(q.autoplay.as_ref().map(|a| (a.mode, a.restart)), Some((super::super::radio::AutoMode::Era, true)), "what was not given stays");
        q.edit_at(Command::Add { from: Default::default(), tracks: vec![in_lane("p3", LANE_AUTOPLAY)], at: None, after: None, how: None }, Some(1)).unwrap();
        q.edit_at(auto(AutoplayMode::Stop), Some(1)).unwrap();
        assert_eq!((ids(&q), q.autoplay.is_none()), ("a b".to_string(), true), "stop: its tracks to come go");
        // Repeat all stops Autoplay the same way.
        q.edit(auto(AutoplayMode::Random)).unwrap();
        q.edit(Command::Add { from: Default::default(), tracks: vec![in_lane("p4", LANE_AUTOPLAY)], at: None, after: None, how: None }).unwrap();
        q.edit_at(Command::Repeat { mode: Repeat::All }, Some(1)).unwrap();
        assert_eq!((ids(&q), q.autoplay.is_none()), ("a b".to_string(), true));
    }

    #[test]
    fn radio_is_a_switch_and_its_off_takes_only_its_inserts_out() {
        let mut q = played(&["a", "b", "c"]);
        q.edit(Command::Radio { on: true, mode: None, seed: None, within: None, by: None, count: None }).unwrap();
        q.edit(auto(AutoplayMode::Similar)).unwrap();
        assert!(q.radio && q.autoplay.is_some(), "Radio lives with Autoplay");
        // Playing a: its inserts after it; Autoplay's tail at the end.
        q.edit_at(Command::Add { from: Default::default(), tracks: vec![in_lane("r1", LANE_RADIO), in_lane("r2", LANE_RADIO)], at: Some(1), after: None, how: None }, Some(0)).unwrap();
        q.edit_at(Command::Add { from: Default::default(), tracks: vec![in_lane("p1", LANE_AUTOPLAY)], at: None, after: None, how: None }, Some(0)).unwrap();
        assert_eq!(ids(&q), "a r1 r2 b c p1");
        q.edit_at(Command::Radio { on: false, mode: None, seed: None, within: None, by: None, count: None }, Some(0)).unwrap();
        assert_eq!((ids(&q), q.radio, q.autoplay.is_some()), ("a b c p1".to_string(), false, true), "off: its inserts out; Autoplay stays");
    }

    fn radio(on: bool, by: Option<RadioBy>, count: Option<u8>) -> Command {
        Command::Radio { on, mode: None, seed: None, within: None, by, count }
    }

    #[test]
    fn radio_keeps_its_choice_and_count_and_another_while_on_takes_the_inserts_out() {
        let mut q = played(&["a", "b"]);
        assert_eq!(q.edit(radio(true, None, None)).unwrap(), radio(true, Some(RadioBy::Track), Some(5)), "told in full");
        q.edit_at(Command::Add { from: Default::default(), tracks: vec![in_lane("r1", LANE_RADIO)], at: Some(1), after: None, how: None }, Some(0)).unwrap();
        let v = q.version;
        q.edit_at(radio(true, Some(RadioBy::Track), Some(5)), Some(0)).unwrap();
        assert_eq!((ids(&q), q.version), ("a r1 b".to_string(), v), "the same again: the inserts stay");
        let told = q.edit_at(radio(true, Some(RadioBy::Era), Some(30)), Some(0)).unwrap();
        assert_eq!(told, radio(true, Some(RadioBy::Era), Some(10)), "at most ten");
        assert_eq!((ids(&q), q.version), ("a b".to_string(), v + 1), "another choice while on: the inserts make way");
        // Off and on again keep the choice; set while off, it changes nothing in the queue.
        q.edit_at(radio(false, None, None), Some(0)).unwrap();
        q.edit_at(radio(false, None, Some(3)), Some(0)).unwrap();
        assert_eq!(q.edit_at(radio(true, None, None), Some(0)).unwrap(), radio(true, Some(RadioBy::Era), Some(3)));
        // Saved with the queue; the defaults are left out.
        let saved = serde_json::to_string(&q).unwrap();
        assert!(saved.contains(r#""radio_by":"era","radio_count":3"#), "{saved}");
        let back: Queue = serde_json::from_str(&saved).unwrap();
        assert_eq!((back.radio, back.radio_by, back.radio_count()), (true, RadioBy::Era, 3));
        assert!(!serde_json::to_string(&played(&["a"])).unwrap().contains("radio"));
    }

    #[test]
    fn skipping_favourites_takes_the_picks_to_come_out_once() {
        let mut q = played(&["a", "b"]);
        q.edit_at(Command::Add { from: Default::default(), tracks: vec![in_lane("r1", LANE_RADIO)], at: Some(1), after: None, how: None }, Some(0)).unwrap();
        q.edit_at(Command::Add { from: Default::default(), tracks: vec![in_lane("p1", LANE_AUTOPLAY)], at: None, after: None, how: None }, Some(0)).unwrap();
        assert_eq!(ids(&q), "a r1 b p1");
        let v = q.version;
        assert_eq!(q.edit_at(Command::SkipFavourites { on: true }, Some(0)).unwrap(), Command::SkipFavourites { on: true });
        assert_eq!((ids(&q), q.version, q.skip_favourites), ("a b".to_string(), v + 1, true), "both lanes' picks to come go");
        q.edit_at(Command::Add { from: Default::default(), tracks: vec![in_lane("r2", LANE_RADIO)], at: Some(1), after: None, how: None }, Some(0)).unwrap();
        let v = q.version;
        q.edit_at(Command::SkipFavourites { on: true }, Some(0)).unwrap();
        assert_eq!((ids(&q), q.version), ("a r2 b".to_string(), v), "the same again: nothing changes");
        assert!(serde_json::to_string(&q).unwrap().contains(r#""skip_favourites":true"#));
        let c: Command = serde_json::from_str(r#"{"command":"skip_favourites","on":false}"#).unwrap();
        assert_eq!(c, Command::SkipFavourites { on: false });
    }

    #[test]
    fn the_radio_command_from_before_the_amendment_is_autoplay() {
        use super::super::radio::{AutoMode, Mode};
        let mut q = played(&["a"]);
        let told = q.edit(Command::Radio { on: true, mode: Some(Mode::AlbumMix), seed: Some(vec!["s".into()]), within: None, by: None, count: None }).unwrap();
        assert!(matches!(told, Command::Autoplay { mode: AutoplayMode::Similar, albums: Some(true), .. }), "{told:?}");
        assert_eq!(q.autoplay.as_ref().map(|a| (a.mode, a.albums, a.seed.clone())), Some((AutoMode::Similar, true, vec!["s".to_string()])));
        assert!(!q.radio, "not the new Radio");
        q.edit(Command::Radio { on: true, mode: Some(Mode::Reshuffle), seed: None, within: None, by: None, count: None }).unwrap();
        assert_eq!(q.autoplay.as_ref().map(|a| (a.restart, a.albums)), Some((true, false)), "Reshuffle is restart");
        q.edit(Command::Radio { on: false, mode: Some(Mode::Similar), seed: None, within: None, by: None, count: None }).unwrap();
        assert!(q.autoplay.is_none());
        let c: Command = serde_json::from_str(r#"{"command":"autoplay","mode":"stop"}"#).unwrap();
        assert_eq!(c, auto(AutoplayMode::Stop));
        let c: Command = serde_json::from_str(r#"{"command":"radio","on":true}"#).unwrap();
        assert_eq!(c, Command::Radio { on: true, mode: None, seed: None, within: None, by: None, count: None });
    }

    #[test]
    fn shuffle_keeps_the_inserts_and_the_tail_and_a_new_play_keeps_both_switches() {
        let tracks: Vec<QueueTrack> = (0..6u32).map(|i| QueueTrack { n: Some(i), ..track(&format!("t{i}")) }).collect();
        let mut q = Queue::default();
        q.edit(Command::play(tracks, 0, 0)).unwrap();
        q.edit(Command::Radio { on: true, mode: None, seed: None, within: None, by: None, count: None }).unwrap();
        q.edit(Command::Autoplay { mode: AutoplayMode::Similar, albums: None, restart: None, seed: Some(vec!["s".into()]), within: Some("c1".into()) }).unwrap();
        q.edit_at(Command::Add { from: Default::default(), tracks: vec![in_lane("x", "next")], at: None, after: None, how: Some("next".into()) }, Some(0)).unwrap();
        q.edit_at(Command::Add { from: Default::default(), tracks: vec![in_lane("r1", LANE_RADIO), in_lane("r2", LANE_RADIO)], at: Some(2), after: None, how: None }, Some(0)).unwrap();
        q.edit_at(Command::Add { from: Default::default(), tracks: vec![in_lane("p1", LANE_AUTOPLAY), in_lane("p2", LANE_AUTOPLAY)], at: None, after: None, how: None }, Some(0)).unwrap();
        // Shuffled once, then again: each time only the own tracks move.
        for round in 1..=2 {
            let Command::Upcoming { tracks: told, .. } = q.edit_at(Command::Shuffle { on: true }, Some(0)).unwrap() else { panic!() };
            let order: Vec<&str> = told.iter().map(|t| t.id.as_str()).collect();
            assert_eq!(&order[..3], ["x", "r1", "r2"], "hand-added, then the inserts, in place (shuffle {round})");
            assert_eq!(&order[order.len() - 2..], ["p1", "p2"], "Autoplay's tail last (shuffle {round})");
        }
        // "Start radio": Autoplay's first tracks played, its seed set first or after; either way the seed stays.
        q.edit(Command::play(vec![in_lane("f1", LANE_AUTOPLAY)], 0, 0)).unwrap();
        assert_eq!(q.autoplay.as_ref().map(|a| a.seed.clone()), Some(vec!["s".to_string()]), "a play of Autoplay's tracks keeps the seed");
        q.edit(Command::play(vec![track("new")], 0, 0)).unwrap();
        assert_eq!(ids(&q), "new");
        assert_eq!(q.autoplay.as_ref().map(|a| (a.seed.is_empty(), a.within.is_none())), Some((true, true)), "the new album its seed, out of the crate");
        assert!(q.radio, "Radio stays on");
    }

    #[test]
    fn repeat_is_the_queues_and_something_new_turns_repeat_one_off() {
        let mut q = played(&["a", "b"]);
        let v = q.version;
        assert_eq!(q.edit(Command::Repeat { mode: Repeat::One }).unwrap(), Command::Repeat { mode: Repeat::One });
        assert_eq!((q.repeat, q.version), (Repeat::One, v), "a setting: the tracks' version stays");
        q.edit(Command::play(vec![track("c")], 0, 0)).unwrap();
        assert_eq!(q.repeat, Repeat::Off, "a new play turns repeat one off");
        q.edit(Command::Repeat { mode: Repeat::All }).unwrap();
        q.edit(Command::play(vec![track("d")], 0, 0)).unwrap();
        assert_eq!(q.repeat, Repeat::All, "repeat all stays");
        let c: Command = serde_json::from_str(r#"{"command":"repeat","mode":"one"}"#).unwrap();
        assert_eq!(c, Command::Repeat { mode: Repeat::One });
    }

    #[test]
    fn a_jump_is_told_by_entry_and_a_stop_empties_the_queue() {
        let mut q = played(&["a", "b"]);
        let b = q.tracks[1].entry.clone();
        assert_eq!(q.edit(Command::Jump { index: Some(1), entry: None }).unwrap(), Command::Jump { index: Some(1), entry: Some(b) });
        assert_eq!(q.version, 1);
        q.edit(Command::Stop { moved_to: None }).unwrap();
        assert!(q.tracks.is_empty());
        assert_eq!(q.version, 2);
    }

    #[test]
    fn a_reported_queue_keeps_its_entries_and_an_older_apps_gets_them_back() {
        let mut q = played(&["a", "b", "a"]);
        let e: Vec<String> = q.tracks.iter().map(|t| t.entry.clone()).collect();
        assert!(!q.adopt(q.tracks.clone()), "the same queue: no change");
        // An app before 0033 moved b first and sends no entries: each track keeps its entry, the twins in order.
        let bare = |ids: &[&str]| ids.iter().map(|i| track(i)).collect::<Vec<_>>();
        assert!(q.adopt(bare(&["b", "a", "a"])));
        assert_eq!(q.tracks.iter().map(|t| t.entry.clone()).collect::<Vec<_>>(), vec![e[1].clone(), e[0].clone(), e[2].clone()]);
        assert_eq!(q.version, 2);
        assert!(!q.adopt(bare(&["b", "a", "a"])), "sent again unchanged");
        // A new track gets a new entry.
        q.adopt(bare(&["b", "a", "a", "c"]));
        assert!(!q.tracks[3].entry.is_empty() && !e.contains(&q.tracks[3].entry));
    }

    #[test]
    fn started_moves_only_when_something_new_starts() {
        let mut q = Queue::default();
        assert_eq!(q.started, 0, "an old queues.json: not known");
        q.edit(Command::play(vec![track("a"), track("b")], 0, 0)).unwrap();
        assert!(q.started > 0, "a play starts it");
        q.started = 1;
        // Radio's and Autoplay's inserts, a track added by hand, a move, repeat, shuffle: none start anything.
        q.edit_at(Command::Add { from: Default::default(), tracks: vec![in_lane("r1", LANE_RADIO)], at: Some(1), after: None, how: None }, Some(0)).unwrap();
        q.edit_at(Command::Add { from: Default::default(), tracks: vec![in_lane("p1", LANE_AUTOPLAY)], at: None, after: None, how: None }, Some(0)).unwrap();
        q.edit_at(Command::Add { from: Default::default(), tracks: vec![in_lane("x", "next")], at: None, after: None, how: Some("next".into()) }, Some(0)).unwrap();
        let b = q.tracks.iter().find(|t| t.id == "b").unwrap().entry.clone();
        q.edit(Command::Move { from: None, to: None, entry: Some(b), after: Some(String::new()) }).unwrap();
        assert_eq!(q.started, 1, "edits and inserts: {}", ids(&q));
        // The player reports the same queue moved about, or with a track taken out: not new.
        let mut reported = q.tracks.clone();
        reported.reverse();
        reported.pop();
        assert!(q.adopt(reported));
        assert_eq!(q.started, 1);
        // It started something of its own: none of the album's entries are left.
        assert!(q.adopt(vec![track("c"), track("d")]));
        assert!(q.started > 1, "a queue of its own");
        // A queue moved whole to this player (a handover) is a play here.
        q.started = 1;
        let moved = q.tracks.clone();
        q.edit_at(Command::play(moved, 0, 0), Some(0)).unwrap();
        assert!(q.started > 1, "handed over");
    }
}
