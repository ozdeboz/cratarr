//! The visuals a sync group shares (decision 0046, amended): whatever shapes the picture (its look and pins, colours,
//! intensity, flash, speed, the grain, the routes, the bands' times; the page decides which keys) is the group's, so
//! changing it on one of the group's devices changes it on all of them. What a device can afford (points, steps,
//! resolution, frame rate) and how bright its screen should be stay its own, and never come here.
//!
//! Shared by everything that shows one player's music: a sync group's devices (kept by the group's leader's name), or
//! a player on its own and any page driving it (by the player's name: the owner's phone driving the salon tablet shows
//! the tablet's). Kept in memory, dropped when the group or the player is gone. Every change gets a version
//! larger than any before, even across a restart (they start from the clock), so a page can ignore what it has seen.
//! A change is merged into what the group has: its top-level keys replace the group's, and its `routes` replace the
//! group's routes key by key, so two people moving different sliders at once do not undo each other. Each change goes
//! out to every page with the group's whole settings (a page that missed one is right again with the next) and who
//! wrote it (`writer`: the page's own id, so it can skip its own echo).

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use serde_json::{Map, Value};
use tokio::sync::broadcast;

/// The most a group's settings may hold, as JSON (bytes); a snapshot's too.
/// 256 KB: a rack converted from an older snapshot (every route a cable) reaches 85 KB, and racks grow as the
/// one engine's modules come in; a group's settings go to its pages whole, which the house network carries easily.
pub const MOST_BYTES: usize = 256 * 1024;
/// The longest writer id.
pub const MOST_WRITER: usize = 64;

/// A group's visuals now.
#[derive(Clone, Debug, Serialize)]
pub struct Shared {
    pub version: u64,
    pub settings: Map<String, Value>,
}

/// A change, as the pages hear it.
#[derive(Clone, Debug, Serialize)]
pub struct Changed {
    pub group: String,
    pub version: u64,
    pub settings: Map<String, Value>,
    pub writer: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    /// Asked to start the group's visuals (`base` 0), but it has them already.
    Exists,
    /// More than `MOST_BYTES`, or a writer id longer than `MOST_WRITER`.
    TooLarge,
}

pub struct GroupVisuals {
    groups: Mutex<HashMap<String, Shared>>,
    last: AtomicU64,
    changed: broadcast::Sender<Changed>,
}

impl Default for GroupVisuals {
    fn default() -> Self {
        Self::new()
    }
}

impl GroupVisuals {
    pub fn new() -> Self {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(1);
        Self { groups: Mutex::default(), last: AtomicU64::new(now), changed: broadcast::channel(32).0 }
    }

    fn groups(&self) -> std::sync::MutexGuard<'_, HashMap<String, Shared>> {
        self.groups.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn get(&self, group: &str) -> Option<Shared> {
        self.groups().get(group).cloned()
    }

    /// `settings` merged into the group's. `base` 0: only where the group has none yet (two devices joining an empty
    /// group at once: the second is refused, and takes the first's). Any other base: the last change wins.
    pub fn put(&self, group: &str, base: u64, settings: Map<String, Value>, writer: &str) -> Result<u64, Refused> {
        if writer.len() > MOST_WRITER || Value::Object(settings.clone()).to_string().len() > MOST_BYTES {
            return Err(Refused::TooLarge);
        }
        let mut groups = self.groups();
        if base == 0 && groups.contains_key(group) {
            return Err(Refused::Exists);
        }
        let mut merged = groups.get(group).map(|g| g.settings.clone()).unwrap_or_default();
        for (key, value) in settings {
            match (key.as_str(), merged.get_mut(&key), value) {
                ("routes", Some(Value::Object(had)), Value::Object(routes)) => had.extend(routes),
                (_, _, value) => {
                    merged.insert(key, value);
                }
            }
        }
        if Value::Object(merged.clone()).to_string().len() > MOST_BYTES {
            return Err(Refused::TooLarge);
        }
        let version = self.last.fetch_add(1, Ordering::SeqCst) + 1;
        groups.insert(group.to_string(), Shared { version, settings: merged.clone() });
        drop(groups);
        let _ = self.changed.send(Changed { group: group.to_string(), version, settings: merged, writer: writer.to_string() });
        Ok(version)
    }

    /// Every group's visuals as they are now, as changes no page wrote (for a page that fell behind).
    pub fn all(&self) -> Vec<Changed> {
        self.groups()
            .iter()
            .map(|(group, g)| Changed { group: group.clone(), version: g.version, settings: g.settings.clone(), writer: String::new() })
            .collect()
    }

    /// The visuals shared by player `from` go to `to`, whose music it now is (a handover; the owner, 2026-10-30): in
    /// place of any `to` had, as a change no page wrote. Nothing when `from` shared none.
    pub fn moved(&self, from: &str, to: &str) {
        if from == to {
            return;
        }
        let mut groups = self.groups();
        let Some(shared) = groups.remove(from) else { return };
        let version = self.last.fetch_add(1, Ordering::SeqCst) + 1;
        groups.insert(to.to_string(), Shared { version, settings: shared.settings.clone() });
        drop(groups);
        let _ = self.changed.send(Changed { group: to.to_string(), version, settings: shared.settings, writer: String::new() });
    }

    /// Only these groups' visuals kept (the others' groups are gone).
    pub fn keep_only(&self, groups: &HashSet<String>) {
        self.groups().retain(|g, _| groups.contains(g));
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Changed> {
        self.changed.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn map(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn a_group_starts_once_and_then_the_last_change_wins() {
        let v = GroupVisuals::new();
        assert!(v.get("Salon tablet").is_none());
        let first = v.put("Salon tablet", 0, map(json!({ "flash": 0.5 })), "a").unwrap();
        assert_eq!(v.put("Salon tablet", 0, map(json!({ "flash": 1.0 })), "b"), Err(Refused::Exists), "the second joiner takes the first's");
        let second = v.put("Salon tablet", 7, map(json!({ "flash": 1.5 })), "b").unwrap();
        assert!(second > first);
        let g = v.get("Salon tablet").unwrap();
        assert_eq!(g.version, second);
        assert_eq!(g.settings["flash"], json!(1.5));
    }

    #[test]
    fn a_change_is_merged_and_its_routes_key_by_key() {
        let v = GroupVisuals::new();
        v.put("g", 0, map(json!({ "flash": 1.0, "speed": 1.0, "routes": { "beat|-|size": 0.08, "loudness|-|size": 1.0 } })), "a").unwrap();
        v.put("g", 1, map(json!({ "speed": 2.0, "routes": { "beat|-|size": 0.5, "hits|0|lights": -1.0 } })), "b").unwrap();
        let s = v.get("g").unwrap().settings;
        assert_eq!(s["flash"], json!(1.0), "a key not sent stays");
        assert_eq!(s["speed"], json!(2.0));
        assert_eq!(s["routes"], json!({ "beat|-|size": 0.5, "loudness|-|size": 1.0, "hits|0|lights": -1.0 }));
        // Routes sent where the group had none (or not an object): taken as they are.
        v.put("h", 0, map(json!({ "routes": { "a": 1 } })), "a").unwrap();
        assert_eq!(v.get("h").unwrap().settings["routes"], json!({ "a": 1 }));
    }

    #[test]
    fn every_change_goes_out_whole_with_its_writer() {
        let v = GroupVisuals::new();
        let mut heard = v.subscribe();
        v.put("g", 0, map(json!({ "flash": 1.0, "speed": 1.0 })), "page-1").unwrap();
        let version = v.put("g", 1, map(json!({ "speed": 0.5 })), "page-2").unwrap();
        let first = heard.try_recv().unwrap();
        assert_eq!(first.writer, "page-1");
        let c = heard.try_recv().unwrap();
        assert_eq!((c.group.as_str(), c.version, c.writer.as_str()), ("g", version, "page-2"));
        assert_eq!(Value::Object(c.settings), json!({ "flash": 1.0, "speed": 0.5 }), "the whole of it, not only the change");
    }

    #[test]
    fn versions_start_from_the_clock_so_a_restart_never_goes_back() {
        let before = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        let v = GroupVisuals::new();
        assert!(v.put("g", 0, Map::new(), "a").unwrap() > before);
    }

    #[test]
    fn too_much_is_refused_and_changes_nothing() {
        let v = GroupVisuals::new();
        v.put("g", 0, map(json!({ "flash": 1.0 })), "a").unwrap();
        let big = map(json!({ "routes": { "x": "y".repeat(MOST_BYTES) } }));
        assert_eq!(v.put("g", 1, big, "a"), Err(Refused::TooLarge));
        assert_eq!(v.put("g", 1, Map::new(), &"w".repeat(MOST_WRITER + 1)), Err(Refused::TooLarge));
        assert_eq!(Value::Object(v.get("g").unwrap().settings), json!({ "flash": 1.0 }));
    }

    #[test]
    fn a_players_visuals_go_where_its_music_is_handed_over() {
        let v = GroupVisuals::new();
        let before = v.put("ThinkPad", 0, map(json!({ "flash": 0.7 })), "a").unwrap();
        v.put("Salon tablet", 0, map(json!({ "flash": 0.1 })), "b").unwrap();
        let mut heard = v.subscribe();
        v.moved("ThinkPad", "Salon tablet");
        assert!(v.get("ThinkPad").is_none());
        let there = v.get("Salon tablet").unwrap();
        assert_eq!(there.settings["flash"], json!(0.7), "in place of its own");
        assert!(there.version > before);
        let c = heard.try_recv().unwrap();
        assert_eq!((c.group.as_str(), c.version, c.writer.as_str()), ("Salon tablet", there.version, ""));
        // None shared: nothing changes.
        v.moved("Kitchen", "Salon tablet");
        assert_eq!(v.get("Salon tablet").unwrap().version, there.version);
    }

    #[test]
    fn a_group_that_is_gone_loses_its_visuals() {
        let v = GroupVisuals::new();
        v.put("Salon tablet", 0, Map::new(), "a").unwrap();
        v.put("Phone", 0, Map::new(), "a").unwrap();
        v.keep_only(&HashSet::from(["Phone".to_string()]));
        assert!(v.get("Salon tablet").is_none());
        assert!(v.get("Phone").is_some());
    }
}
