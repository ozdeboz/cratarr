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
    if folder == 0 {
        return Some(rel);
    }
    let folders = cat.folders();
    crate::playlists::relative_to(folders.first()?, &folders.get(folder as usize)?.join(rel))
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
    fn ids_and_names() {
        assert_eq!(id_of("Chill"), "pl-Chill");
        assert_eq!(name_of("pl-Chill"), Some("Chill"));
        assert_eq!(name_of("pl-"), None);
        assert_eq!(new_name("Chill", &s(&["chill"])), "Chill 2");
        assert_eq!(new_name("a/b", &[]), "a-b");
    }
}
