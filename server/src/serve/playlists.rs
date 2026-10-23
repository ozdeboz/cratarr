//! Cratarr's playlists as the OpenSubsonic API shows them (decision 0030): the `.m3u` files, through the playlist
//! service, so an edit from a Subsonic app is an edit of the file with its undo and its sync to other servers. A
//! playlist's id is `pl-` and its name. Lines are turned into songs through the catalogue, and songs into lines the
//! way the playlist sync does (a track's path relative to the Music folder), never from what a client sent. Lines
//! whose file is missing stay in the file and are not shown; positions a client sends count the shown songs only.

use super::ids;
use crate::catalogue::Catalogue;

pub const PLAYLIST: &str = "pl-";

pub fn id_of(name: &str) -> String {
    format!("{PLAYLIST}{name}")
}

pub fn name_of(id: &str) -> Option<&str> {
    id.strip_prefix(PLAYLIST).filter(|n| !n.is_empty())
}

/// A playlist line's song id, when its file is in the catalogue.
pub fn song_of_line(cat: &Catalogue, line: &str) -> Option<String> {
    let (folder, rel) = cat.place_of_line(line)?;
    cat.track_of(folder, &rel).map(ids::song)
}

/// A song's playlist line: its path relative to the Music folder (as `app::sync`'s `line`).
pub fn line_of_song(cat: &Catalogue, song: &str) -> Option<String> {
    let track: i64 = song.strip_prefix(ids::SONG)?.parse().ok()?;
    let (folder, rel) = cat.file_of(track)?;
    cat.line_of(folder, &rel)
}

/// The lines of a playlist after removing the shown songs at `remove` (positions among the shown songs) and adding
/// `add` (lines) at the end. Lines not shown (missing files) stay where they are.
pub fn edited(lines: &[String], shown: &[bool], remove: &[usize], add: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(lines.len() + add.len());
    let mut position = 0;
    for (line, is_shown) in lines.iter().zip(shown) {
        if *is_shown {
            let gone = remove.contains(&position);
            position += 1;
            if gone {
                continue;
            }
        }
        out.push(line.clone());
    }
    out.extend(add.iter().cloned());
    out
}

/// How long a call that took every song out of a playlist waits for the call that puts the new list in.
pub const REFILL_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// What an `updatePlaylist` call does to a playlist's file.
#[derive(Debug, PartialEq)]
pub enum Step {
    Unchanged,
    Write(Vec<String>),
    /// Every song taken out and none added: held for the call that puts the new list in (`Held`).
    Hold(Vec<String>),
}

/// One `updatePlaylist` call on a playlist whose file holds `lines` (`shown`: which are songs the library has), after
/// the emptying held for it, if any (`emptied`: its lines once emptied, the lines of missing files only).
pub fn update_step(lines: &[String], shown: &[bool], emptied: Option<Vec<String>>, remove: &[usize], add: &[String]) -> Step {
    let new = match emptied {
        // The second call of a two-call edit edits the emptied playlist; no song is shown in it.
        Some(emptied) => edited(&emptied, &vec![false; emptied.len()], remove, add),
        None => {
            let new = edited(lines, shown, remove, add);
            let songs = shown.iter().filter(|s| **s).count();
            if songs > 0 && add.is_empty() && new.len() + songs == lines.len() {
                return Step::Hold(new);
            }
            new
        }
    };
    if new == lines { Step::Unchanged } else { Step::Write(new) }
}

/// Playlists a client emptied that are not written yet. Symfonium edits a playlist in two calls, every song out and
/// then the new list in; written one by one, the edit would be two undos, and the playlist empty for a moment on every
/// server it is synced to. So the emptying waits `REFILL_WAIT` for the call that refills it, and the two are written
/// as one. Nothing following, the emptying is written on its own (`take_ticket`).
#[derive(Default)]
pub struct Held {
    next: std::sync::atomic::AtomicU64,
    held: std::sync::Mutex<std::collections::HashMap<String, Emptied>>,
}

pub struct Emptied {
    ticket: u64,
    /// The playlist's lines when it was emptied (the file as it still is).
    pub before: Vec<String>,
    /// Its lines emptied.
    pub after: Vec<String>,
}

impl Held {
    fn map(&self) -> std::sync::MutexGuard<'_, std::collections::HashMap<String, Emptied>> {
        self.held.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Hold a playlist's emptying; returns its ticket.
    pub fn hold(&self, name: &str, before: Vec<String>, after: Vec<String>) -> u64 {
        let ticket = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.map().insert(name.to_string(), Emptied { ticket, before, after });
        ticket
    }

    /// The emptying held for a playlist, taken: the call that refills it writes both.
    pub fn take(&self, name: &str) -> Option<Emptied> {
        self.map().remove(name)
    }

    /// The emptying held under `ticket`, taken, when no later call took or replaced it.
    pub fn take_ticket(&self, name: &str, ticket: u64) -> Option<Emptied> {
        let mut map = self.map();
        if map.get(name).is_some_and(|e| e.ticket == ticket) { map.remove(name) } else { None }
    }

    /// A playlist's lines as a client sees them: emptied while its emptying is held.
    pub fn shown(&self, name: &str, lines: Vec<String>) -> Vec<String> {
        self.map().get(name).filter(|e| e.before == lines).map(|e| e.after.clone()).unwrap_or(lines)
    }
}

/// A free, usable file name for a playlist a client made.
pub fn new_name(wanted: &str, taken: &[String]) -> String {
    let name = crate::playlists::usable_name(wanted);
    crate::playlists::free_name(&name, |n| taken.iter().any(|t| t.eq_ignore_ascii_case(n)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn edits_count_the_shown_songs_and_keep_the_missing_lines() {
        let lines = s(&["a", "gone", "b", "c"]);
        let shown = [true, false, true, true];
        assert_eq!(edited(&lines, &shown, &[1], &s(&["d"])), s(&["a", "gone", "c", "d"]), "position 1 is b, not the missing line");
        assert_eq!(edited(&lines, &shown, &[], &[]), lines);
        assert_eq!(edited(&lines, &shown, &[0, 2], &[]), s(&["gone", "b"]));
    }

    #[test]
    fn a_two_call_edit_is_one_write() {
        // Symfonium adding "c" to a playlist with a missing file: every song out, then the new list in.
        let lines = s(&["a", "gone", "b"]);
        let shown = [true, false, true];
        let first = update_step(&lines, &shown, None, &[0, 1], &[]);
        assert_eq!(first, Step::Hold(s(&["gone"])), "emptied: held, the missing line kept");
        let second = update_step(&lines, &shown, Some(s(&["gone"])), &[], &s(&["a", "b", "c"]));
        assert_eq!(second, Step::Write(s(&["gone", "a", "b", "c"])), "one write from the file as it was");
        let same = update_step(&lines, &shown, Some(s(&["gone"])), &[], &s(&["a", "b"]));
        assert_eq!(same, Step::Write(s(&["gone", "a", "b"])), "the missing line goes first, as it does on its own");
        assert_eq!(update_step(&lines, &shown, Some(s(&["gone"])), &[0], &[]), Step::Write(s(&["gone"])), "emptied after all");
    }

    #[test]
    fn other_edits_are_written_at_once() {
        let lines = s(&["a", "b"]);
        let shown = [true, true];
        assert_eq!(update_step(&lines, &shown, None, &[1], &[]), Step::Write(s(&["a"])), "one song out");
        assert_eq!(update_step(&lines, &shown, None, &[0, 1], &s(&["c"])), Step::Write(s(&["c"])), "replaced in one call");
        assert_eq!(update_step(&lines, &shown, None, &[], &[]), Step::Unchanged);
        assert_eq!(update_step(&s(&["gone"]), &[false], None, &[0], &[]), Step::Unchanged, "no song to take out");
    }

    #[test]
    fn a_held_emptying_is_taken_once_by_its_ticket() {
        let held = Held::default();
        let first = held.hold("Chill", s(&["a"]), vec![]);
        assert_eq!(held.shown("Chill", s(&["a"])), Vec::<String>::new(), "shown emptied while held");
        assert_eq!(held.shown("Chill", s(&["x"])), s(&["x"]), "unless the file changed since");
        let second = held.hold("Chill", s(&["a"]), vec![]);
        assert!(held.take_ticket("Chill", first).is_none(), "replaced by a later emptying");
        assert_eq!(held.take_ticket("Chill", second).unwrap().before, s(&["a"]));
        assert!(held.take("Chill").is_none(), "taken once");
    }

    #[test]
    fn ids_and_names() {
        assert_eq!(id_of("Chill"), "pl-Chill");
        assert_eq!(name_of("pl-Chill"), Some("Chill"));
        assert_eq!(name_of("pl-"), None);
        assert_eq!(new_name("Chill", &s(&["chill"])), "Chill 2");
        assert_eq!(new_name("a/b", &[]), "a-b");
    }
}
