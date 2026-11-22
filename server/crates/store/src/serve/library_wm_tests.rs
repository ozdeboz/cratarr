//! SCENARIOS-WM's served half: every fact field the library reads goes through the one tag rule (WM.2), a stamped file reads as
//! one without the tag on every reader (WM.3), and a stamp never wins a folder's vote (WM.16).

use super::testing::*;
use super::*;
use crate::incoming::TrackFacts;
use crate::naming::Tags;

const STAMP: &str = "www.electronicfresh.com";

/// What every reader of one file gives: the served song's own fields (`song`), and the pairing's (`TrackFacts::of`).
fn read(r: &ServedRow, folder: i64) -> (String, String, bool, String, Vec<String>, Option<String>, Vec<String>, String) {
    let s = song(r, folder, &FolderRules::default());
    let t = TrackFacts::of(&Tags(r.tags.clone()), &r.rel, r.duration_ms);
    (s.title, s.album, s.album_named, s.album_artist, s.files_credit.names, s.artist_credit, s.album_artists, t.shown().to_string())
}

/// What an album gives its record, its hashes left out (T-WM-2: those read the raw tags).
fn record_facts(a: &crate::catalogue::AlbumCopy) -> (String, String, Vec<String>, Vec<(String, String)>, String, String, Vec<String>, usize) {
    (a.title.clone(), a.artist.clone(), a.artists.clone(), a.labels.clone(), a.facts.title.clone(), a.facts.artist.clone(), a.facts.tracks.iter().map(|t| format!("{t:?}")).collect(), a.tracks.len())
}

/// WM.2: a file whose every fact field is a site's address reads each as no value, where the library, the record pass and the
/// pairing read it; a file with no TITLE stands by its name, cleaned ("02"); a comment, a web address frame, a website tag and the
/// MusicBrainz ids are read as today, the rule never touching them. Red first: each read as written.
#[test]
fn every_fact_field_reads_through_the_rule() {
    let facts = ["title", "album", "artist", "artists", "albumartist", "albumartists", "label", "organization", "publisher", "catalognumber", "composer"];
    let mut tags: Vec<(&str, &str)> = facts.iter().map(|f| (*f, STAMP)).collect();
    let release = "00000000-0000-4000-8000-000000000301";
    tags.extend([("comment", "Visit https://kelpe.bandcamp.com"), ("url", "https://kelpe.bandcamp.com"), ("website", "https://kelpe.bandcamp.com"), ("musicbrainz_albumid", release)]);
    let first = ServedRow { record: Some(9), ..row(1, "Kelpe/Drome/01 Drome.flac", &tags) };
    let second = ServedRow { record: Some(9), ..row(2, "Kelpe/Drome/02 - www.electronicfresh.com.flac", &[]) };

    let (title, album, named, album_artist, artists, credit, album_artists, paired) = read(&first, 1);
    assert_eq!(title, "01 Drome", "a stamped TITLE is none: the file's name stands");
    assert_eq!((album.as_str(), named), ("[Unknown Album]", false), "as a file with no ALBUM");
    assert_eq!(album_artist, UNKNOWN_ARTIST);
    assert!(artists.is_empty() && credit.is_none() && album_artists == [UNKNOWN_ARTIST], "no artist named by a stamp: {album_artists:?}");
    assert_eq!(paired, "01 Drome", "the pairing reads its name's titles");
    assert_eq!(TrackFacts::of(&Tags(first.tags.clone()), &first.rel, None).named, ["01 Drome", "Drome"]);
    assert_eq!(read(&second, 1).0, "02", "its name, the stamp cut");
    assert_eq!(TrackFacts::of(&Tags(Vec::new()), &second.rel, None).named, ["02"]);

    let copies = album_copies(&[in_folder(1, vec![first.clone(), second])], &FolderRules::default());
    assert_eq!(copies.len(), 1);
    let a = &copies[0];
    assert!(a.labels.is_empty(), "no label named by a stamp: {:?}", a.labels);
    assert!(a.facts.title.is_empty(), "no title key from a stamp");
    assert!([&a.title, &a.artist].into_iter().chain(&a.artists).all(|v| !v.contains("electronicfresh")), "{a:?}");
    assert_eq!(a.facts.release.as_deref(), Some(release), "a MusicBrainz id is read as today");

    for (name, value) in [("comment", "Visit https://kelpe.bandcamp.com"), ("url", "https://kelpe.bandcamp.com"), ("website", "https://kelpe.bandcamp.com"), ("musicbrainz_albumid", release)] {
        assert_eq!(crate::catalogue::fact_value(name, value).as_deref(), Some(value), "{name}: no fact field, never touched");
        assert_eq!(first.first(name), Some(value), "its raw row as read");
    }
}

/// The real case's folder (SCENARIOS-WM's fixture): `ss-140`'s four MP3s, every ALBUM the site, no ALBUMARTIST, ARTIST "Red Axes,
/// Polo & Pan", no track numbers, titles "T1", "T2 www.", "T3" and none; or its twin, the stamps removed.
fn mumia(stamped: bool) -> Vec<ServedRow> {
    let files: [(&str, Option<&str>, i64); 4] = if stamped {
        [("b.mp3", Some("T1"), 190_000), ("c.mp3", Some("T2 www."), 260_000), ("a.mp3", Some("T3"), 330_000), ("T4 - www.electronicfresh.com.mp3", None, 400_000)]
    } else {
        [("b.mp3", Some("T1"), 190_000), ("c.mp3", Some("T2"), 260_000), ("a.mp3", Some("T3"), 330_000), ("T4.mp3", None, 400_000)]
    };
    files
        .iter()
        .zip(1..)
        .map(|((file, title, ms), track)| {
            let mut tags = vec![("artist", "Red Axes, Polo & Pan")];
            if stamped {
                tags.push(("album", STAMP));
            }
            if let Some(t) = title {
                tags.push(("title", t));
            }
            ServedRow { duration_ms: Some(*ms as u64), record: Some(3211), ..row(track, &format!("soulseek/Polo & Pan/Mumia (2023) 2/{file}"), &tags) }
        })
        .collect()
}

/// WM.3: every reader gives the stamped folder's files the answer it gives their twin: the album no value, as a file with none;
/// the album artist the artist credit; the titles T1-T4 (the fourth by its name, WM.25). The served album and song rows alike.
/// Red first: the album read "www.electronicfresh.com", the titles "T2 www." and "T4 - www.electronicfresh.com".
#[test]
fn a_stamped_file_reads_as_one_without_the_tag() {
    let (stamped, twin) = (mumia(true), mumia(false));
    for (s, t) in stamped.iter().zip(&twin) {
        assert_eq!(read(s, 5), read(t, 5), "{} against its twin {}", s.rel, t.rel);
    }
    let (title, album, named, album_artist, ..) = read(&stamped[3], 5);
    assert_eq!((title.as_str(), album.as_str(), named, album_artist.as_str()), ("T4", "[Unknown Album]", false, "Red Axes, Polo & Pan"));
    assert_eq!(stamped.iter().map(|r| read(r, 5).7).collect::<Vec<_>>(), ["T1", "T2", "T3", "T4"]);

    let rules = FolderRules::default();
    let (a, b) = (album_copies(&[in_folder(5, stamped.clone())], &rules), album_copies(&[in_folder(5, twin.clone())], &rules));
    assert_eq!((a.len(), b.len()), (1, 1));
    assert_eq!(record_facts(&a[0]), record_facts(&b[0]), "what the record pass and its credits read");

    // The served library, built of each alone: its album and song rows, as the device's index and every list read them.
    let served = |rows: Vec<ServedRow>| {
        let lib = built_with(&[(5, Kind::Incoming)], &[in_folder(5, rows)]);
        let db = lib.db();
        let album: (String, Option<String>) = db.query_row("SELECT name, artist FROM albums", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        let mut songs: Vec<(String, String, Option<String>)> =
            db.prepare("SELECT title, album, album_artist FROM songs ORDER BY title").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect();
        songs.sort();
        (album, songs)
    };
    let (s, t) = (served(stamped), served(twin));
    assert_eq!(s, t);
    assert!(!format!("{s:?}").contains("electronicfresh") && !format!("{s:?}").contains("www."), "never the stamp: {s:?}");
}

/// A folder of IN's files dropped by hand (no item, no placement, no ids), each ARTIST "Polo & Pan", titles unalike: (path, ALBUM).
fn folder(files: &[(&str, &str)]) -> Vec<crate::catalogue::AlbumCopy> {
    let rows: Vec<ServedRow> = files
        .iter()
        .zip(1..)
        .map(|((file, album), track)| row(track, &format!("Polo & Pan/Drop/{file}"), &[("album", album), ("artist", "Polo & Pan"), ("title", &format!("Song {track}"))]))
        .collect();
    album_copies(&[in_folder(1, rows)], &FolderRules::default())
}

/// WM.16 (T-WM-10): a folder's album is the one most of its files name after the tag rule; a stamp is no value, so it has no vote
/// and never wins a tie, though its files come first by path; a tie between two values still goes to the first file by path.
/// Red first: (b), (c), the stamped group winning the tie by path and the record titled by the site.
#[test]
fn a_stamp_never_wins_a_folders_tie() {
    let one = |albums: Vec<crate::catalogue::AlbumCopy>| -> (usize, String, usize) {
        let a = &albums[0];
        (albums.len(), a.title.clone(), a.tracks.len())
    };
    assert_eq!(one(folder(&[("1.mp3", STAMP), ("2.mp3", STAMP), ("3.mp3", "Mumia II"), ("4.mp3", "Mumia II")])), (1, "Mumia II".into(), 4), "(b)");
    assert_eq!(one(folder(&[("1.mp3", STAMP), ("2.mp3", "Mumia II")])), (1, "Mumia II".into(), 2), "(c)");
    // (d), a guard: two real values tie, and the first file by path decides, as before.
    assert_eq!(one(folder(&[("1.mp3", "Mumia 2"), ("2.mp3", "Mumia 2"), ("3.mp3", "Mumia II"), ("4.mp3", "Mumia II")])), (1, "Mumia 2".into(), 4), "(d)");
}

/// WM.16 (a), a pin: three files naming "Mumia II" and one stamped are one album, "Mumia II", of four.
#[test]
fn a_stamped_file_is_its_folders_record() {
    let albums = folder(&[("1.mp3", "Mumia II"), ("2.mp3", "Mumia II"), ("3.mp3", "Mumia II"), ("4.mp3", STAMP)]);
    assert_eq!((albums.len(), albums[0].title.as_str(), albums[0].tracks.len()), (1, "Mumia II", 4));
}
