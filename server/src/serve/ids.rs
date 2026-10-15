//! Ids the OpenSubsonic API hands out (decision 0030). Songs are Cratarr's own track ids, which survive moves
//! (decision 0011); albums and artists are hashes of their tags, so they survive moves, renames of folders and a
//! rebuilt catalogue. Each song also has a persistent id from its tags, by which user data finds it again when the
//! catalogue is rebuilt and its track ids renumbered.
//!
//! The hashes and persistent ids are translated from Navidrome (`model/id/id.go`, `model/metadata/persistent_ids.go`,
//! `utils/str/str.go`, `consts/consts.go`), Copyright the Navidrome authors, GPL-3.0-or-later
//! (https://github.com/navidrome/navidrome). Changes: Rust; the default specs only (Navidrome makes them configurable);
//! no library id prefix, so an album keeps its id when it moves from Incoming into Music; Cratarr's tag names
//! (Navidrome's `musicbrainz_trackid` is the release track id, `musicbrainz_releasetrackid` here); ids carry a type
//! prefix (`al-`, `ar-`), as gonic's do, so a bare id says what it is.

use md5::{Digest, Md5};

use crate::catalogue::ServedRow;

pub const SONG: &str = "tr-";
pub const ALBUM: &str = "al-";
pub const ARTIST: &str = "ar-";

/// A song's id from its Cratarr track id.
pub fn song(track: i64) -> String {
    format!("{SONG}{track}")
}

/// The Cratarr track id in a song id.
pub fn track_of(id: &str) -> Option<i64> {
    id.strip_prefix(SONG)?.parse().ok()
}

/// Navidrome's `NewHash`: md5 over each part followed by a zero-width space, as 22 base62 characters.
pub fn hash(parts: &[&str]) -> String {
    let mut h = Md5::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update("\u{200b}".as_bytes());
    }
    base62(u128::from_be_bytes(h.finalize().into()))
}

/// Navidrome's `Encode`: a 128-bit value in base 62 (0-9, a-z, A-Z, Go's digit order), zero-padded to 22 characters.
fn base62(mut v: u128) -> String {
    const DIGITS: &[u8; 62] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut out = [b'0'; 22];
    for slot in out.iter_mut().rev() {
        *slot = DIGITS[(v % 62) as usize];
        v /= 62;
    }
    String::from_utf8(out.to_vec()).unwrap_or_default()
}

/// Navidrome's `str.Clear`: typographic quotes and dashes as their plain ASCII ones.
pub fn clear(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '‘' | '’' | '‛' | '′' => '\'',
            '＂' | '〃' | 'ˮ' | 'ײ' | '᳓' | '″' | '‶' | '˶' | 'ʺ' | '“' | '”' | '˝' | '‟' => '"',
            '‐' | '–' | '—' | '−' | '―' => '-',
            c => c,
        })
        .collect()
}

/// An artist's id from its name: the same name (ignoring case and typographic quotes) is the same artist.
pub fn artist(name: &str) -> String {
    format!("{ARTIST}{}", artist_hash(name))
}

fn artist_hash(name: &str) -> String {
    hash(&[&clear(&name.to_lowercase())])
}

/// The first field group of a spec with any value, its values joined by `\` (Navidrome's `computePID`).
fn first_group(groups: &[Vec<String>]) -> String {
    groups.iter().find(|g| g.iter().any(|v| !v.is_empty())).map(|g| g.join("\\")).unwrap_or_default()
}

/// The album's persistent id, Navidrome's default `musicbrainz_albumid|albumartistid,album,albumversion,releasedate`.
fn album_pid(row: &ServedRow, album_artist: &str) -> String {
    let release_date = row.first("releasedate").or(row.first("date")).unwrap_or("");
    first_group(&[
        vec![row.first("musicbrainz_albumid").unwrap_or("").to_string()],
        vec![
            artist_hash(album_artist),
            clear(&row.first("album").unwrap_or("").to_lowercase()),
            // Album version: no tag Cratarr reads holds it.
            String::new(),
            release_date.to_string(),
        ],
    ])
}

/// An album's id: the hash of its persistent id. `album_artist` is the album artist as shown (the song's artist when
/// the file names none). Outside the Music folder (`folder` 0) the folder goes in front, as Navidrome's library id:
/// an upgrade waiting in Incoming is not the album it would replace, and once kept into Music it has that id.
pub fn album(row: &ServedRow, album_artist: &str, folder: i64) -> String {
    let pid = album_pid(row, album_artist);
    let pid = if folder == 0 { pid } else { format!("{folder}\\{pid}") };
    format!("{ALBUM}{}", hash(&[&pid]))
}

/// A song's persistent id, Navidrome's default `musicbrainz_trackid|albumid,discnumber,tracknumber,title`.
pub fn song_pid(row: &ServedRow, album_artist: &str) -> String {
    let album_id = hash(&[&album_pid(row, album_artist)]);
    hash(&[&first_group(&[
        vec![row.first("musicbrainz_releasetrackid").unwrap_or("").to_string()],
        vec![
            album_id,
            row.first("discnumber").unwrap_or("").to_string(),
            row.first("tracknumber").unwrap_or("").to_string(),
            row.first("title").unwrap_or("").to_string(),
        ],
    ])])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(tags: &[(&str, &str)]) -> ServedRow {
        ServedRow { tags: tags.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect(), ..ServedRow::default() }
    }

    #[test]
    fn hashes_are_navidromes() {
        // Expected values computed separately (Python: md5 with the same separator, Go's base-62 digit order).
        assert_eq!(hash(&[""]), "6l2eDCFmY72hS7z8186KZd");
        assert_eq!(hash(&["björk"]), "5vcUvYmuWJAgyxLXEUmaPz");
        assert_eq!(base62(0), "0000000000000000000000");
        assert_eq!(base62(61), "000000000000000000000Z");
        assert_eq!(base62(62), "0000000000000000000010");
        assert_eq!(base62(u128::MAX), "7N42dgm5tFLK9N8MT7fHC7");
    }

    #[test]
    fn the_same_artist_whatever_the_case_or_quotes() {
        assert_eq!(artist("Guns N’ Roses"), artist("guns n' roses"));
        assert_ne!(artist("Björk"), artist("Bjork"));
    }

    #[test]
    fn an_album_is_its_release_else_its_artist_title_and_date() {
        let tagged = row(&[("album", "Homogenic"), ("musicbrainz_albumid", "b8a7c51f-362c-4dcb-a259-bc6e0095f0a6"), ("date", "1997")]);
        let retitled = row(&[("album", "Homogenic (Remaster)"), ("musicbrainz_albumid", "b8a7c51f-362c-4dcb-a259-bc6e0095f0a6")]);
        assert_eq!(album(&tagged, "Björk", 0), album(&retitled, "Björk", 0), "the release id decides");
        assert_ne!(album(&tagged, "Björk", 0), album(&tagged, "Björk", 1), "Incoming keeps its own");
        let plain = row(&[("album", "Homogenic"), ("date", "1997")]);
        assert_eq!(album(&plain, "Björk", 0), album(&row(&[("album", "HOMOGENIC"), ("date", "1997")]), "björk", 0));
        assert_ne!(album(&plain, "Björk", 0), album(&row(&[("album", "Homogenic"), ("date", "1998")]), "Björk", 0));
    }

    #[test]
    fn a_song_is_its_release_track_else_its_place_on_the_album() {
        let a = row(&[("album", "X"), ("tracknumber", "1"), ("title", "One")]);
        let b = row(&[("album", "X"), ("tracknumber", "2"), ("title", "One")]);
        assert_ne!(song_pid(&a, "A"), song_pid(&b, "A"));
        assert_eq!(song_pid(&a, "A"), song_pid(&a.clone(), "A"));
        assert_eq!(track_of(&song(42)), Some(42));
        assert_eq!(track_of("al-42"), None);
    }
}
