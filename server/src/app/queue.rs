//! A player's queue as Cratarr holds it (decision 0033): entries with ids of their own, edited by entry, with a version
//! that goes up on every change. Pure: Play On keeps one per player, saves it and tells the player.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::ToggleError;
use super::playon::{Command, QueueTrack};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Queue {
    pub version: u64,
    pub tracks: Vec<QueueTrack>,
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

    /// Makes an edit, and returns it as the player is told: by entry, and by place too, for apps before decision 0033.
    /// Commands that do not touch the queue come back as they were.
    pub fn edit(&mut self, command: Command) -> Result<Command, ToggleError> {
        let told = match command {
            Command::Play { from, tracks: Some(tracks), index, position_ms, shuffle, paused } => {
                self.tracks = with_entries(tracks);
                Command::Play { from, tracks: Some(self.tracks.clone()), index, position_ms, shuffle, paused }
            }
            Command::Add { tracks, at, after } => {
                let tracks = with_entries(tracks);
                let p = match after.as_deref() {
                    Some("") => 0,
                    Some(a) => self.at(a).ok_or_else(missing)? + 1,
                    None => at.unwrap_or(self.tracks.len()).min(self.tracks.len()),
                };
                let after = self.anchor(p);
                self.tracks.splice(p..p, tracks.iter().cloned());
                Command::Add { tracks, at: Some(p), after: Some(after) }
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
            Command::Remove { index, entry } => {
                let i = self.place(entry.as_deref(), index)?;
                let gone = self.tracks.remove(i);
                Command::Remove { index: Some(i), entry: Some(gone.entry) }
            }
            // Not an edit: the entry that plays is the player's to say.
            Command::Jump { index, entry } => {
                let i = self.place(entry.as_deref(), index)?;
                return Ok(Command::Jump { index: Some(i), entry: Some(self.tracks[i].entry.clone()) });
            }
            Command::Stop => {
                self.tracks.clear();
                Command::Stop
            }
            other => return Ok(other),
        };
        self.version += 1;
        Ok(told)
    }

    /// The queue a player reports, taken as it is: the player holds what plays (decision 0033, step 1). Tracks it
    /// sends without entries (an app from before 0033) keep the entries they had here, in order, so an entry keeps
    /// its id through moves. `true` when the queue changed.
    pub fn adopt(&mut self, reported: Vec<QueueTrack>) -> bool {
        let mut known: Vec<Option<&QueueTrack>> = self.tracks.iter().map(Some).collect();
        let tracks: Vec<QueueTrack> = reported
            .into_iter()
            .map(|mut t| {
                if t.entry.is_empty() {
                    let same = known.iter_mut().find(|k| k.is_some_and(|k| k.source == t.source && k.id == t.id));
                    t.entry = same.and_then(Option::take).map_or_else(new_entry, |k| k.entry.clone());
                }
                t
            })
            .collect();
        if tracks == self.tracks {
            return false;
        }
        self.tracks = tracks;
        self.version += 1;
        true
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
        let told = q.edit(Command::Add { tracks: vec![track("x"), track("y")], at: None, after: Some(a.clone()) }).unwrap();
        assert_eq!(ids(&q), "a x y b");
        let Command::Add { tracks, at, after } = told else { panic!() };
        assert_eq!((at, after), (Some(1), Some(a)));
        assert!(tracks.iter().all(|t| !t.entry.is_empty()), "the player is told their entries");
        q.edit(Command::Add { tracks: vec![track("z")], at: None, after: None }).unwrap();
        assert_eq!(ids(&q), "a x y b z", "at the end");
        let Command::Add { after, .. } = q.edit(Command::Add { tracks: vec![track("w")], at: Some(0), after: None }).unwrap() else { panic!() };
        assert_eq!((ids(&q).as_str(), after), ("w a x y b z", Some(String::new())));
    }

    #[test]
    fn a_jump_is_told_by_entry_and_a_stop_empties_the_queue() {
        let mut q = played(&["a", "b"]);
        let b = q.tracks[1].entry.clone();
        assert_eq!(q.edit(Command::Jump { index: Some(1), entry: None }).unwrap(), Command::Jump { index: Some(1), entry: Some(b) });
        assert_eq!(q.version, 1);
        q.edit(Command::Stop).unwrap();
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
}
