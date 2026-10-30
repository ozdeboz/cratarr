//! A playlist's entries by id, and its edits as operations on them (0.12, decision 0087). Pure.
//!
//! - Every line has an entry id: a page makes an added line's, Cratarr gives each other line one (`matched`). The
//!   `.m3u` stays the truth: lines it gained elsewhere get new ids, lines it lost become removed entries.
//! - Membership merges as a set: an add of an entry known (there, or removed) changes nothing; a remove of one not there
//!   changes nothing. A removed entry is remembered with the entry before it then, so an add after it goes after its
//!   last known neighbour; an add after an entry never known goes at the end.
//! - The order: the last move by its time wins; a move older than the entry's last move or add changes nothing.

use std::collections::{HashMap, VecDeque};

/// One line of a playlist, by its entry id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    /// The line as the `.m3u` has it (Music-relative).
    pub line: String,
}

/// An entry removed: its line, the entry before it then (none: it was first), and when (epoch ms).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gone {
    pub line: String,
    pub after: Option<String>,
    pub at: i64,
}

/// A playlist's entries: those there in order, when each was last placed (an add or a move, epoch ms; 0 for a line
/// Cratarr gave an id to), and those removed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entries {
    pub live: Vec<Entry>,
    pub placed: HashMap<String, i64>,
    pub gone: HashMap<String, Gone>,
}

/// One operation, its track's line found already (never a path a page sent).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Do {
    /// After `after` (none: at the start).
    Add { entry: String, line: String, after: Option<String> },
    Remove { entry: String },
    /// After `after` (none: at the start).
    Move { entry: String, after: Option<String> },
}

/// An entry id Cratarr makes: `s` and 12 hex digits from the system's random source.
pub fn new_id() -> String {
    let mut b = [0u8; 6];
    getrandom::fill(&mut b).expect("the operating system's random source");
    format!("s{}", b.iter().map(|x| format!("{x:02x}")).collect::<String>())
}

/// An entry id or an operation's id as a page may send it: a short plain token (a UUID).
pub fn plain_id(v: &str) -> bool {
    (1..=64).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

impl Entries {
    pub fn lines(&self) -> Vec<String> {
        self.live.iter().map(|e| e.line.clone()).collect()
    }

    pub fn ids(&self) -> Vec<String> {
        self.live.iter().map(|e| e.id.clone()).collect()
    }

    /// The playlist as the file now has `lines`, matched by order and content: the n-th copy of a line keeps the n-th
    /// copy's id, a line not there before gets a new one (`mint`, placed `now`), and an entry whose line is gone is
    /// removed (`now`), remembered after the entry before it. Nothing changes when the lines are the same.
    pub fn matched(&mut self, lines: &[String], now: i64, mut mint: impl FnMut() -> String) {
        if self.live.iter().map(|e| e.line.as_str()).eq(lines.iter().map(String::as_str)) {
            return;
        }
        let mut by_line: HashMap<&str, VecDeque<&str>> = HashMap::new();
        for e in &self.live {
            by_line.entry(e.line.as_str()).or_default().push_back(e.id.as_str());
        }
        let mut live = Vec::with_capacity(lines.len());
        let mut fresh = Vec::new();
        for line in lines {
            let id = match by_line.get_mut(line.as_str()).and_then(VecDeque::pop_front) {
                Some(id) => id.to_string(),
                None => {
                    let id = mint();
                    fresh.push(id.clone());
                    id
                }
            };
            live.push(Entry { id, line: line.clone() });
        }
        let kept: std::collections::HashSet<&str> = live.iter().map(|e| e.id.as_str()).collect();
        let mut gone = Vec::new();
        for (i, e) in self.live.iter().enumerate() {
            if !kept.contains(e.id.as_str()) {
                let after = i.checked_sub(1).map(|j| self.live[j].id.clone());
                gone.push((e.id.clone(), Gone { line: e.line.clone(), after, at: now }));
            }
        }
        for (id, g) in gone {
            self.placed.remove(&id);
            self.gone.insert(id, g);
        }
        for id in fresh {
            self.placed.insert(id, now);
        }
        self.live = live;
    }

    /// Applies one operation made at `at` (the device's time) now (`now`, epoch ms). Whether anything changed.
    pub fn apply(&mut self, op: &Do, at: i64, now: i64) -> bool {
        match op {
            Do::Add { entry, line, after } => {
                if self.position(entry).is_some() || self.gone.contains_key(entry) {
                    return false;
                }
                let place = self.place(after.as_deref());
                self.insert(place, Entry { id: entry.clone(), line: line.clone() });
                self.placed.insert(entry.clone(), at);
                true
            }
            Do::Remove { entry } => {
                let Some(i) = self.position(entry) else { return false };
                let e = self.live.remove(i);
                let after = i.checked_sub(1).map(|j| self.live[j].id.clone());
                self.placed.remove(entry);
                self.gone.insert(e.id, Gone { line: e.line, after, at: now });
                true
            }
            Do::Move { entry, after } => {
                let Some(i) = self.position(entry) else { return false };
                if self.placed.get(entry).is_some_and(|&p| p >= at) {
                    return false;
                }
                let place = match self.place(after.as_deref()) {
                    // A neighbour never known: it stays where it is.
                    Place::End => return false,
                    Place::After(id) if id == *entry => return false,
                    p => p,
                };
                let e = self.live.remove(i);
                self.insert(place, e);
                self.placed.insert(entry.clone(), at);
                true
            }
        }
    }

    fn position(&self, entry: &str) -> Option<usize> {
        self.live.iter().position(|e| e.id == entry)
    }

    /// Where "after `after`" is now: after it while it is there, after its last known neighbour once removed, at the
    /// end for an entry never known.
    fn place(&self, after: Option<&str>) -> Place {
        let mut at = after;
        // Each removed entry followed at most once: a chain never loops, but a bad record must not hang.
        for _ in 0..=self.gone.len() {
            let Some(id) = at else { return Place::Start };
            if self.position(id).is_some() {
                return Place::After(id.to_string());
            }
            match self.gone.get(id) {
                Some(g) => at = g.after.as_deref(),
                None => return Place::End,
            }
        }
        Place::Start
    }

    fn insert(&mut self, place: Place, e: Entry) {
        let i = match place {
            Place::Start => 0,
            Place::End => self.live.len(),
            Place::After(id) => self.position(&id).map_or(self.live.len(), |i| i + 1),
        };
        self.live.insert(i, e);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Place {
    Start,
    After(String),
    End,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counter() -> impl FnMut() -> String {
        let mut n = 0;
        move || {
            n += 1;
            format!("s{n}")
        }
    }

    fn lines(x: &[&str]) -> Vec<String> {
        x.iter().map(|s| s.to_string()).collect()
    }

    fn of(x: &[&str]) -> Entries {
        let mut e = Entries::default();
        e.matched(&lines(x), 1, counter());
        e
    }

    fn ids(e: &Entries) -> Vec<&str> {
        e.live.iter().map(|x| x.id.as_str()).collect()
    }

    fn add(entry: &str, line: &str, after: Option<&str>) -> Do {
        Do::Add { entry: entry.into(), line: line.into(), after: after.map(str::to_string) }
    }

    fn mv(entry: &str, after: Option<&str>) -> Do {
        Do::Move { entry: entry.into(), after: after.map(str::to_string) }
    }

    fn rm(entry: &str) -> Do {
        Do::Remove { entry: entry.into() }
    }

    #[test]
    fn lines_keep_their_ids_by_order_and_content_repeats_copy_by_copy() {
        let mut e = of(&["a", "b", "a", "c"]);
        assert_eq!(ids(&e), ["s1", "s2", "s3", "s4"]);
        // Reordered, the second a taken out, d added: written elsewhere.
        e.matched(&lines(&["c", "a", "b", "d"]), 50, || "s9".to_string());
        assert_eq!(e.live.iter().map(|x| x.line.as_str()).collect::<Vec<_>>(), ["c", "a", "b", "d"]);
        assert_eq!(ids(&e), ["s4", "s1", "s2", "s9"], "c, the first a and b keep theirs; d is new");
        assert_eq!(e.placed["s9"], 50);
        let g = &e.gone["s3"];
        assert_eq!((g.line.as_str(), g.after.as_deref(), g.at), ("a", Some("s2"), 50), "the second a is gone, after b");
        // The same lines again: nothing changes.
        let before = e.clone();
        e.matched(&lines(&["c", "a", "b", "d"]), 60, counter());
        assert_eq!(e, before);
    }

    #[test]
    fn adds_and_removes_merge_as_a_set_and_are_safe_to_send_again() {
        let mut e = of(&["a", "b"]);
        assert!(e.apply(&add("p1", "x", Some("s1")), 100, 1000));
        assert_eq!(ids(&e), ["s1", "p1", "s2"]);
        assert!(!e.apply(&add("p1", "x", Some("s1")), 100, 1001), "sent again");
        assert!(e.apply(&rm("p1"), 110, 1002));
        assert!(!e.apply(&rm("p1"), 110, 1003), "removed again");
        assert!(!e.apply(&add("p1", "x", Some("s1")), 100, 1004), "an add of a removed entry, late: still removed");
        assert!(!e.apply(&rm("nobody"), 120, 1005));
        assert_eq!(ids(&e), ["s1", "s2"]);
    }

    #[test]
    fn an_add_after_a_removed_entry_goes_after_its_last_known_neighbour_and_after_one_never_known_at_the_end() {
        let mut e = of(&["a", "b", "c"]);
        // Another device removed b, then c.
        e.apply(&rm("s2"), 100, 1000);
        e.apply(&rm("s3"), 101, 1001);
        // This device, offline, added after c: c followed b, b followed a.
        assert!(e.apply(&add("p1", "x", Some("s3")), 90, 1002));
        assert_eq!(ids(&e), ["s1", "p1"]);
        assert!(e.apply(&add("p2", "y", Some("never")), 91, 1003));
        assert_eq!(ids(&e), ["s1", "p1", "p2"]);
        assert!(e.apply(&add("p3", "z", None), 92, 1004));
        assert_eq!(ids(&e), ["p3", "s1", "p1", "p2"], "after none: at the start");
    }

    #[test]
    fn the_last_move_by_its_time_wins() {
        let mut e = of(&["a", "b", "c"]);
        assert!(e.apply(&mv("s1", Some("s3")), 200, 1000));
        assert_eq!(ids(&e), ["s2", "s3", "s1"]);
        assert!(!e.apply(&mv("s1", None), 150, 1001), "an older move of the same entry changes nothing");
        assert!(!e.apply(&mv("s1", Some("s3")), 200, 1002), "the same move sent again");
        assert!(e.apply(&mv("s1", None), 250, 1003));
        assert_eq!(ids(&e), ["s1", "s2", "s3"]);
        assert!(!e.apply(&mv("s2", Some("never")), 300, 1004), "after one never known: it stays");
        assert!(!e.apply(&mv("gone", None), 300, 1005), "an entry not there");
        assert!(!e.apply(&mv("s2", Some("s2")), 300, 1006), "after itself");
        // After one since removed: after its last known neighbour.
        e.apply(&rm("s3"), 310, 1007);
        assert!(e.apply(&mv("s1", Some("s3")), 320, 1008));
        assert_eq!(ids(&e), ["s2", "s1"]);
        // An entry added at 400 is not moved by a move made before it.
        e.apply(&add("p1", "x", None), 400, 1009);
        assert!(!e.apply(&mv("p1", Some("s1")), 390, 1010));
    }

    #[test]
    fn ids_from_a_page_are_short_plain_tokens() {
        assert!(plain_id("6f1c2a9e-0d3b-4c55-9a8e-2b7f0c1d4e5f"));
        assert!(!plain_id("") && !plain_id("a/b") && !plain_id(&"x".repeat(65)));
        let (a, b) = (new_id(), new_id());
        assert!(a.starts_with('s') && a.len() == 13 && a != b);
    }
}
