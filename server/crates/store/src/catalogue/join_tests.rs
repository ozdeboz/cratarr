//! The arrival's join to its record (`Catalogue::join_copies`, F11, F40) as one pairing for identity: `assign` with X3's title
//! rule, a length a cost and never a refusal (SCENARIOS-PAIR01 PAIR01.19 as amended 2026-11-21; SCENARIOS-VR01 VR44, VR45, RV-20).

use rusqlite::{OptionalExtension, params};

use super::Catalogue;
use crate::settings::LibraryKind;

const KINDS: [(i64, LibraryKind); 5] =
    [(0, LibraryKind::Main), (1, LibraryKind::Incoming), (2, LibraryKind::Own), (4, LibraryKind::Main), (5, LibraryKind::Incoming)];

/// A had record with one copy in catalogue folder `folder` (0 MUSIC, main; 1 IN, Incoming), credited to Glass Harbour (one
/// artist id for every record of these tests): each file (its number, its title, its length in ms) a track of it.
fn joinable(cat: &Catalogue, title: &str, folder: i64, files: &[(Option<u32>, String, i64)]) -> i64 {
    let db = cat.db();
    db.execute("INSERT INTO records (born, title, artist) VALUES (1, ?1, 'Glass Harbour')", [title]).unwrap();
    let record = db.last_insert_rowid();
    let artist: i64 = match db.query_row("SELECT id FROM artists WHERE key = 'glassharbour'", [], |r| r.get(0)).optional().unwrap() {
        Some(id) => id,
        None => {
            db.execute("INSERT INTO artists (born, name, key) VALUES (1, 'Glass Harbour', 'glassharbour')", []).unwrap();
            db.last_insert_rowid()
        }
    };
    db.execute("INSERT INTO record_artists (record, artist) VALUES (?1, ?2)", params![record, artist]).unwrap();
    for (i, (number, name, ms)) in files.iter().enumerate() {
        db.execute("INSERT INTO tracks (born, record, disc, number) VALUES (1, ?1, 1, ?2)", params![record, i as i64 + 1]).unwrap();
        let track = db.last_insert_rowid();
        let rel = format!("{title} {folder} {record}/{:02}.flac", i + 1);
        db.execute("INSERT INTO files (folder, rel, size, mtime, track, duration_ms) VALUES (?1, ?2, 1, 1, ?3, ?4)", params![folder, rel, track, ms]).unwrap();
        let mut tags = vec![("title", name.clone())];
        if let Some(n) = number {
            tags.push(("tracknumber", n.to_string()));
        }
        for (n, (name, value)) in tags.into_iter().enumerate() {
            db.execute("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (?1, ?2, ?3, ?4, ?4, ?5)", params![folder, rel, n as i64, name, value]).unwrap();
        }
    }
    record
}

fn live(cat: &Catalogue, record: i64) -> i64 {
    cat.record(record).unwrap().unwrap().id
}

/// PAIR01.19 (as amended): the join still refuses another title. Two copies whose track 3 is "Pier" in one and "Breakwater" in
/// the other, lengths equal, stay two records; "Pier (Live)" against "Pier" likewise, whichever side says it. A guard the join's
/// move must keep (today's `incoming::pair` refuses them too): a wrong pair moves play history.
#[test]
fn the_join_still_refuses_another_title() {
    let cat = Catalogue::open(None, Vec::new()).unwrap();
    let sea_wall = |third: &str| vec![(Some(1), "Landing".to_string(), 240_000), (Some(2), "Low Water".to_string(), 260_000), (Some(3), third.to_string(), 250_000)];
    for (mine, other, what) in [("Pier", "Breakwater", "another title"), ("Pier", "Pier (Live)", "a version word the arrival says"), ("Pier (Live)", "Pier", "a version word the copy says")] {
        let music = joinable(&cat, "Sea Wall", 0, &sea_wall(mine));
        let arrival = joinable(&cat, "Sea Wall", 1, &sea_wall(other));
        assert_eq!(cat.join_copies(music, arrival, &KINDS).unwrap(), None, "{what}: two records");
        assert_eq!(cat.join_copies(arrival, music, &KINDS).unwrap(), None, "{what}: either way round");
        assert_eq!((live(&cat, music), live(&cat, arrival)), (music, arrival), "{what}: no record merged, no play history moved");
        cat.execute_for_tests(&format!("UPDATE files SET gone_at = 1 WHERE track IN (SELECT id FROM tracks WHERE record IN ({music}, {arrival}));"));
    }
}

/// VR44 (Mary Lattimore, "Hundreds Of Days"; C13 settled, RV-20, RV-13): the arrival's join pairs every track through `assign`
/// with X3's title rule, a length a cost and never a refusal: track 2's 48 s by decode keeps nothing apart, and track 7's "(Bonus
/// Track)" names no version; (c) with no TRACKNUMBER and no recording, it still joins by its title: one record, two copies,
/// decided by the pairing, never the files' numbers. VR45's Laband: track 18's 120 s against 637.8 s joins too. Red first: (c) at
/// the base (`incoming::pair` pairs a numberless "(Bonus Track)" title by no pass: two records).
#[test]
fn hundreds_of_days_is_one_record_whole_as_the_owner_picks() {
    let cat = Catalogue::open(None, Vec::new()).unwrap();
    let days = |bonus: Option<u32>, two_ms: i64, bracket: bool| -> Vec<(Option<u32>, String, i64)> {
        (1..=7u32)
            .map(|n| match n {
                2 => (Some(2), "Track 2".to_string(), two_ms),
                7 => (bonus, if bracket { "Track 7 (Bonus Track)".to_string() } else { "Track 7".to_string() }, 300_000),
                n => (Some(n), format!("Track {n}"), 180_000 + i64::from(n) * 11_000),
            })
            .collect()
    };
    for (case, bonus) in [("(b)", Some(7)), ("(c)", None)] {
        // The MP3 in Music, its track 2 served 48.4 s past the FLAC's; the FLAC in Incoming, as Lidarr placed it.
        let mp3 = joinable(&cat, "Hundreds Of Days", 0, &days(bonus, 498_800, true));
        let flac = joinable(&cat, "Hundreds of Days", 1, &days(Some(7), 450_400, false));
        assert_eq!(cat.join_copies(mp3, flac, &KINDS).unwrap(), Some((flac, mp3)), "{case}: one record, two copies, the main library's record kept");
        assert_eq!(live(&cat, flac), mp3, "{case}: the FLAC's id answers as the record it went into");
        let on: i64 = cat
            .db()
            .query_row(
                "SELECT count(DISTINCT t.record) FROM files f JOIN tracks t ON t.id = f.track WHERE f.rel LIKE 'Hundreds%' AND f.gone_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(on, 1, "{case}: every file of both copies on one record's tracks (play history moved with them)");
        cat.execute_for_tests("UPDATE files SET gone_at = 1 WHERE rel LIKE 'Hundreds%';");
    }
    // VR45: Felix Laband's FLAC, its track 18 cut to 120 s against the MP3's 637.8 s: one record by the join.
    let stairs = |last: i64| (1..=18u32).map(|n| (Some(n), format!("Stair {n}"), if n == 18 { last } else { 180_000 + i64::from(n) * 9_000 })).collect::<Vec<_>>();
    let music = joinable(&cat, "4/4 Down The Stairs", 0, &stairs(637_800));
    let arrival = joinable(&cat, "4/4 Down the Stairs", 1, &stairs(120_000));
    assert_eq!(cat.join_copies(music, arrival, &KINDS).unwrap(), Some((arrival, music)), "the cut track is a cost, never a refusal");
}

/// FA18 (RG2-wide, F40; M11, H4): any main or Incoming pair of one tracklist joins: MUSIC and MORE, IN and IN2, MUSIC and IN, and two
/// records in MUSIC itself (`al-62` in `A/Thin Walls/`, `al-63` in `B/Thin Walls (FLAC)/`), whose files become one copy of the
/// joined record; MUSIC and OWN-A stay two. The record with a main library's copy stays, else the older. Red first: RG2 joined an
/// Incoming-alone record to a main one only (MUSIC and MORE, IN and IN2, MUSIC twice: two records). (A waiting upload's exception is
/// UP03's, LB: the uploads library is not told apart by kind here.)
#[test]
fn the_two_copies_join_reads_any_main_or_incoming_pair() {
    let cat = Catalogue::open(None, Vec::new()).unwrap();
    let thin = || (1..=9u32).map(|n| (Some(n), format!("Wall {n}"), 180_000 + i64::from(n) * 5_000)).collect::<Vec<_>>();
    let mut n = 0;
    let mut pair = |a: i64, b: i64| {
        n += 1;
        (joinable(&cat, &format!("Thin Walls {n}"), a, &thin()), joinable(&cat, &format!("Thin Walls {n}"), b, &thin()))
    };
    let joined = |cat: &Catalogue, (x, y): (i64, i64)| live(cat, x) == live(cat, y);
    let (music_more, in_in2, music_in, music_own) = (pair(0, 4), pair(1, 5), pair(0, 1), pair(0, 2));
    for (p, what) in [(music_more, "MUSIC and MORE"), (in_in2, "IN and IN2"), (music_in, "MUSIC and IN")] {
        assert!(cat.join_copies(p.0, p.1, &KINDS).unwrap().is_some(), "{what}: one record");
        assert!(joined(&cat, p), "{what}");
    }
    assert_eq!(live(&cat, music_more.1), music_more.0, "the older stays");
    assert_eq!(cat.join_copies(music_own.0, music_own.1, &KINDS).unwrap(), None, "MUSIC and OWN-A: two (RG2)");
    // Two records in MUSIC itself: one record, one copy in MUSIC of both directories' files, two files on each track.
    let (a, b) = pair(0, 0);
    assert_eq!(cat.join_copies(a, b, &KINDS).unwrap(), Some((b, a)), "a main library with itself is a main pair");
    let files_per_track: Vec<i64> = cat
        .db()
        .prepare("SELECT count(*) FROM files f JOIN tracks t ON t.id = f.track WHERE t.record = ?1 AND f.gone_at IS NULL GROUP BY t.id")
        .unwrap()
        .query_map([a], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(files_per_track, vec![2; 9], "two files on each track, one copy (H4)");
}
