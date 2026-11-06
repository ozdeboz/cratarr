//! Ids the OpenSubsonic API hands out (decision 0030). Songs are Cratarr's own track ids, which survive moves
//! (decision 0011). Albums are records (0100): `al-<record>`, the catalogue's number for life, whatever copies it has
//! and wherever they lie. Artists too: `ar-<id>`, one name one artist until references split them. Each song also has a
//! persistent id from its tags, by which user data finds it again when the catalogue is rebuilt and its track ids
//! renumbered.
//!
//! An album's hash (`album`) is no id any more: it gathers one folder's files of an album, what its tags say, and it is
//! what the first start reads the old ids by.
//!
//! The hashes and persistent ids are translated from Navidrome (`model/id/id.go`, `model/metadata/persistent_ids.go`,
//! `utils/str/str.go`, `consts/consts.go`), Copyright the Navidrome authors, GPL-3.0-or-later
//! (https://github.com/navidrome/navidrome). Changes: Rust; the default specs only (Navidrome makes them configurable);
//! no library id prefix, so an album keeps its hash when it moves from Incoming into Music; Cratarr's tag names
//! (Navidrome's `musicbrainz_trackid` is the release track id, `musicbrainz_releasetrackid` here); ids carry a type
//! prefix (`al-`, `ar-`), as gonic's do, so a bare id says what it is.

use md5::{Digest, Md5};

use crate::catalogue::ServedRow;

pub use cratarr_rules::ids::{ALBUM, ARTIST, SONG, song, track_of};

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

/// An artist's id as it is served: their number in the catalogue, for life.
pub fn artist(id: i64) -> String {
    format!("{ARTIST}{id}")
}

/// What makes two names one artist: the same name, ignoring case and typographic quotes.
pub fn artist_key(name: &str) -> String {
    clear(&name.to_lowercase())
}

/// The name's hash, which was an artist's id until 0.16: what a persistent id is made of.
fn artist_hash(name: &str) -> String {
    hash(&[&artist_key(name)])
}

/// The id an artist went by until 0.16, from their key: what the migration moves from.
pub fn old_artist(key: &str) -> String {
    format!("{ARTIST}{}", hash(&[key]))
}

/// Whether an album's or an artist's id is one for life, a number: a hash from before 0.16 is 22 letters and digits.
pub fn for_life(id: &str) -> bool {
    let number = id.strip_prefix(ALBUM).or_else(|| id.strip_prefix(ARTIST));
    number.is_some_and(|n| (1..=15).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The first field group of a spec with any value, its values joined by `\` (Navidrome's `computePID`).
fn first_group(groups: &[Vec<String>]) -> String {
    groups.iter().find(|g| g.iter().any(|v| !v.is_empty())).map(|g| g.join("\\")).unwrap_or_default()
}

/// The album's persistent id, Navidrome's default `musicbrainz_albumid|albumartistid,album,albumversion,releasedate`.
pub(super) fn album_pid(row: &ServedRow, album_artist: &str) -> String {
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

/// A record's id as it is served: its number in the catalogue, for life.
pub fn record(record: i64) -> String {
    format!("{ALBUM}{record}")
}

/// The highest number named in `text` as a record's or an artist's id for life, `al-<digits>` or `ar-<digits>`, 0 for
/// none: in a column, a key (`like:al-12`, `album:cratarr/al-12`) or a whole JSON store. A hash is no number: it runs on
/// in letters, and 22 digits of one, if it ever came to that, are no id anyone was given.
pub fn highest_named(text: &str) -> i64 {
    let word = |c: u8| c.is_ascii_alphanumeric();
    let bytes = text.as_bytes();
    let mut top = 0;
    for prefix in [ALBUM, ARTIST] {
        for (at, _) in text.match_indices(prefix) {
            if at > 0 && word(bytes[at - 1]) {
                continue;
            }
            let start = at + prefix.len();
            let digits = bytes[start..].iter().take_while(|b| b.is_ascii_digit()).count();
            let cut_short = bytes.get(start + digits).is_some_and(|b| word(*b));
            if (1..=15).contains(&digits) && !cut_short {
                top = top.max(text[start..start + digits].parse().unwrap_or(0));
            }
        }
    }
    top
}

/// What gathers one folder's files of an album, never an id: the hash of its persistent id. `album_artist` is the album
/// artist as shown (the song's artist when the file names none). Outside folder 0 the folder goes in front, as Navidrome's
/// library id, so two folders' files stay two copies. It keeps the form of the ids albums had until 0.16, as the first start
/// finds what was kept under those by it.
pub(super) fn album(row: &ServedRow, album_artist: &str, folder: i64) -> String {
    album_at(&album_pid(row, album_artist), folder)
}

/// `album`, from its persistent id: what it was known by, or would have been, in `folder`.
pub fn album_at(pid: &str, folder: i64) -> String {
    let at = if folder == 0 { hash(&[pid]) } else { hash(&[&format!("{folder}\\{pid}")]) };
    format!("{ALBUM}{at}")
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
    fn an_artists_old_id_is_the_hash_of_their_key() {
        assert_eq!(old_artist(&artist_key("Björk")), "ar-5vcUvYmuWJAgyxLXEUmaPz");
        assert_eq!(old_artist(&artist_key("BJÖRK")), format!("{ARTIST}{}", artist_hash("björk")));
    }

    #[test]
    fn an_id_for_life_is_a_number_and_a_hash_is_not() {
        assert!(for_life("ar-12") && for_life("al-3000001"));
        assert!(!for_life("ar-5vcUvYmuWJAgyxLXEUmaPz") && !for_life("al-1234567890123456789012") && !for_life("ar-") && !for_life("tr-12"));
    }

    #[test]
    fn the_same_artist_whatever_the_case_or_quotes() {
        assert_eq!(artist_key("Guns N’ Roses"), artist_key("guns n' roses"));
        assert_ne!(artist_key("Björk"), artist_key("Bjork"));
    }

    #[test]
    fn a_folders_files_gather_by_their_release_else_their_artist_title_and_date() {
        let tagged = row(&[("album", "Homogenic"), ("musicbrainz_albumid", "b8a7c51f-362c-4dcb-a259-bc6e0095f0a6"), ("date", "1997")]);
        let retitled = row(&[("album", "Homogenic (Remaster)"), ("musicbrainz_albumid", "b8a7c51f-362c-4dcb-a259-bc6e0095f0a6")]);
        assert_eq!(album(&tagged, "Björk", 0), album(&retitled, "Björk", 0), "the release id decides");
        assert_ne!(album(&tagged, "Björk", 0), album(&tagged, "Björk", 1), "each folder gathers its own");
        let plain = row(&[("album", "Homogenic"), ("date", "1997")]);
        assert_eq!(album(&plain, "Björk", 0), album(&row(&[("album", "HOMOGENIC"), ("date", "1997")]), "björk", 0));
        assert_ne!(album(&plain, "Björk", 0), album(&row(&[("album", "Homogenic"), ("date", "1998")]), "Björk", 0));
    }

    /// The ids in `tests/fixtures/0.15` are written down, not made, as nothing here can run 0.15: each must be what 0.15 made
    /// of the tags beside it, or the migration is proved on ids nobody ever had. The tables are the tag's, so what came after
    /// must be missing and what the migration moves must hold a row.
    #[test]
    fn every_id_the_data_of_0_15_names_is_the_hash_of_its_tags() {
        use std::collections::{BTreeMap, BTreeSet};
        let (catalogue, userdata) = (include_str!("../../tests/fixtures/0.15/catalogue.sql"), include_str!("../../tests/fixtures/0.15/userdata.sql"));
        let stores = [include_str!("../../tests/fixtures/0.15/crates.json"), include_str!("../../tests/fixtures/0.15/users/bob/crates.json")];
        let more = [include_str!("../../tests/fixtures/0.15/queues.json"), include_str!("../../tests/fixtures/0.15/journal.sql"), include_str!("../../tests/fixtures/0.15/convert-queue.json"), include_str!("../../tests/fixtures/0.15/track-intake.json")];
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(catalogue).unwrap();
        db.execute_batch(userdata).unwrap();

        let mut files: BTreeMap<(i64, String), ServedRow> = BTreeMap::new();
        let mut st = db.prepare("SELECT folder, rel, name, value FROM tags ORDER BY folder, rel, n").unwrap();
        let tags = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))).unwrap();
        for tag in tags {
            let (folder, rel, name, value) = tag.unwrap();
            files.entry((folder, rel)).or_default().tags.push((name, value));
        }
        let (mut albums, mut artists) = (BTreeSet::new(), BTreeSet::new());
        for ((folder, _), row) in &files {
            let by = row.first("albumartist").unwrap();
            // A row can name an album by any place it has had: a folder's id was its place in a list, and Music is one of them.
            albums.extend([album(row, by, *folder), album(row, by, 0), album(row, by, 1), album(row, by, 2)]);
            artists.extend(row.all("artist").chain([by]).map(|n| format!("{ARTIST}{}", artist_hash(n))));
        }
        // A hash is 22 letters and digits: a shorter word that happens to hold the prefix is no id.
        let named = |text: &str, prefix: &str| -> BTreeSet<String> {
            let word = |c: char| c.is_ascii_alphanumeric();
            let hashes = text.match_indices(prefix).filter(|(at, _)| !text[..*at].ends_with(word)).map(|(at, _)| &text[at..at + prefix.len() + text[at + prefix.len()..].chars().take_while(|c| word(*c)).count()]);
            hashes.filter(|id| id.len() == prefix.len() + 22).map(str::to_string).collect()
        };
        for text in std::iter::once(userdata).chain(stores) {
            let (album_ids, artist_ids) = (named(text, ALBUM), named(text, ARTIST));
            assert!(!album_ids.is_empty() && !artist_ids.is_empty(), "the ids there are found");
            assert!(album_ids.is_subset(&albums), "no such album: {:?}", album_ids.difference(&albums).collect::<Vec<_>>());
            assert!(artist_ids.is_subset(&artists), "no such artist: {:?}", artist_ids.difference(&artists).collect::<Vec<_>>());
        }
        for text in more {
            assert!(named(text, ALBUM).is_subset(&albums), "no such album: {text}");
        }

        let tables: BTreeSet<String> = db.prepare("SELECT name FROM sqlite_master WHERE type = 'table'").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        for later in ["records", "handovers", "migration_report", "playlist", "playlist_import", "playlist_item", "playlist_moved", "preference"] {
            assert!(!tables.contains(later), "{later} came after 0.15");
        }
        let columns = |table: &str| -> BTreeSet<String> { db.prepare(&format!("SELECT name FROM pragma_table_info('{table}')")).unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect() };
        assert!(["record", "disc", "number"].iter().all(|c| !columns("tracks").contains(*c)) && !columns("folders").contains("home") && !columns("files").contains("came"));
        let moved = [
            "added", "album_review", "album_tags", "annotation", "record_overrides", "classical_marks", "album_matches", "album_complete", "intake_heard", "record_references",
            "renumber_matched", "discogs_genres", "bandcamp_found", "request", "artist_person", "related_artists", "discography", "imported", "words", "words_asked", "discogs_facts",
            "release_types", "change_at", "change_log", "bookmark", "plays", "scrobble_queue", "track_words", "app_queue", "visual_rating", "playlist_entry_id", "audiomuse", "source_ids",
        ];
        for table in moved {
            assert!(db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get::<_, i64>(0)).unwrap() > 0, "{table} holds a row");
        }
        // A track retired before the migration has no file: the line and the plays on it are kept, missing.
        let retired: i64 = db.query_row("SELECT count(*) FROM tracks WHERE retired_at IS NOT NULL AND id NOT IN (SELECT track FROM files)", [], |r| r.get(0)).unwrap();
        assert_eq!(retired, 1);
    }

    /// What a lost catalogue's floor is taken from: an id for life wherever it is written, and never a hash that begins
    /// with digits, or one album's name would lift every id by a number nobody chose.
    #[test]
    fn the_highest_id_named_is_found_in_any_text_and_a_hash_is_none() {
        assert_eq!(highest_named("al-12"), 12);
        assert_eq!(highest_named("album:cratarr/al-7"), 7);
        assert_eq!(highest_named("like:ar-30"), 30);
        assert_eq!(highest_named(r#"{"of":"al-4","seeds":["ar-9","al-2"]}"#), 9);
        assert_eq!(highest_named("al-1 al-3 al-2"), 3);
        assert_eq!(highest_named("al-5vcUvYmuWJAgyxLXEUmaPz"), 0, "a hash");
        assert_eq!(highest_named("al-0000000000000000000012"), 0, "a hash of digits alone");
        assert_eq!(highest_named("tr-99 normal-12 al-"), 0, "no record's");
    }

    #[test]
    fn a_song_is_its_release_track_else_its_place_on_the_album() {
        let a = row(&[("album", "X"), ("tracknumber", "1"), ("title", "One")]);
        let b = row(&[("album", "X"), ("tracknumber", "2"), ("title", "One")]);
        assert_ne!(song_pid(&a, "A"), song_pid(&b, "A"));
        assert_eq!(song_pid(&a, "A"), song_pid(&a.clone(), "A"));
        assert_eq!(track_of(&song(42)), Some(42));
        assert_eq!(track_of("al-42"), None);
        assert_eq!(record(42), "al-42");
    }
}
