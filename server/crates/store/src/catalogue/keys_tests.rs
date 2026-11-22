//! SCENARIOS-COUPLING COU.9 (T-COU-6, T-COU-18, T-COU-19): a song's persistent key is its record's settled album and its paired
//! track; the rows on a song carry its key, refreshed at each build (a handover's among them); at a fresh start of the track ids
//! everything keyed to a song follows it by the key, worked out again from the files the same way. On real folders, through the
//! catalogue's pass and the served library's build.

use std::collections::{HashMap, HashSet};

use super::{Catalogue, FolderRules};
use super::wm_tests::{Folders, IN, MUSIC};
use crate::serve::copies::Kind;
use crate::serve::library::Library;
use crate::userdata::{AppQueue, Kind as Item, UserData};

const TWO_PIERS: &str = "Glass Harbour/Two Piers (2018)";
const DEEP_HARBOUR: &str = "Commodo & Alfa Mist/Deep Harbour";

/// The served library built from `cat`, its records given by the catalogue's pass.
fn built(cat: &Catalogue) -> Library {
    let lib = Library::open(None).unwrap();
    lib.set_kinds(vec![(MUSIC, Kind::Main), (IN, Kind::Main)]);
    let (_, print) = lib.change(cat, &FolderRules::default()).unwrap();
    lib.build_from(cat, &FolderRules::default(), &print).unwrap();
    lib
}

/// The files as Two Piers and Deep Harbour hold them now, read into a catalogue made anew: the tagged files with their tags, the
/// better copy as it was placed, the placed untagged files with none (no placement is known any more).
fn read_afresh(cat: &Catalogue, better: &str, landing: &[(&str, &str)]) {
    cat.walk_all();
    cat.pretend_read_for_tests();
    cat.tag_for_tests(MUSIC, better, landing);
    for (n, title) in [("01", "Breakwater"), ("02", "Sea Wall")] {
        let number = n.trim_start_matches('0');
        cat.tag_for_tests(IN, &format!("{DEEP_HARBOUR}/{n} {title}.flac"), &[("album", "Deep Harbour"), ("albumartist", "Commodo & Alfa Mist"), ("title", title), ("tracknumber", number)]);
    }
    cat.identify().unwrap();
}

fn song_of(cat: &Catalogue, folder: i64, rel: &str) -> String {
    crate::serve::ids::song(cat.track_of(folder, rel).expect("a track"))
}

/// COU.9: "Two Piers" `al-301`'s untagged placed `01 Pier.flac` (COU.1 (b)) beside its tagged `02 Landing.flac`, and an untagged file
/// placed into "Deep Harbour" `al-291` named as its neighbours (WM.23 (b)). Alice's rows on both (plays, play history, a heart, a
/// rating, a bookmark), bob's playlist lines on them, carol's saved app queue, all written before; Landing's file then handed over to
/// a better copy (its rows carry its key now, T-COU-6). The track ids started afresh: the keys worked out again from the files, the
/// two untagged files' keys apart; every row follows its own track; carol's queue names the old ids; a key two songs share moves
/// nothing. Red first: a pid from the tags (the two untagged files one pid, given to its first song only); the play history and the
/// playlist lines followed nothing.
#[test]
fn a_songs_rows_follow_its_record_and_paired_track() {
    let f = Folders::new("cou9");
    let (pier, landing, undertow) = (format!("{TWO_PIERS}/01 Pier.flac"), format!("{TWO_PIERS}/02 Landing.flac"), format!("{DEEP_HARBOUR}/03 Undertow.flac"));
    let two_piers = [("album", "Two Piers"), ("albumartist", "Glass Harbour"), ("title", "Landing"), ("tracknumber", "2"), ("date", "2018")];
    f.add(MUSIC, &[(landing.as_str(), two_piers.to_vec())]);
    f.add(IN, &[(format!("{DEEP_HARBOUR}/01 Breakwater.flac").as_str(), vec![("album", "Deep Harbour"), ("albumartist", "Commodo & Alfa Mist"), ("title", "Breakwater"), ("tracknumber", "1")])]);
    f.add(IN, &[(format!("{DEEP_HARBOUR}/02 Sea Wall.flac").as_str(), vec![("album", "Deep Harbour"), ("albumartist", "Commodo & Alfa Mist"), ("title", "Sea Wall"), ("tracknumber", "2")])]);
    f.pass();
    let (r301, r291) = (f.record_of(MUSIC, &landing).unwrap(), f.record_of(IN, &format!("{DEEP_HARBOUR}/01 Breakwater.flac")).unwrap());
    // The two untagged files, placed (COU.1 (b), WM.23 (b)): said before they appear, untouched.
    f.cat.place_file(MUSIC, &pier, r301, None, (1, 1)).unwrap();
    f.cat.place_file(IN, &undertow, r291, None, (1, 3)).unwrap();
    f.add(MUSIC, &[(pier.as_str(), Vec::new())]);
    f.add(IN, &[(undertow.as_str(), Vec::new())]);
    f.pass();
    let lib = built(&f.cat);
    let (pier_song, undertow_song, landing_song) = (song_of(&f.cat, MUSIC, &pier), song_of(&f.cat, IN, &undertow), song_of(&f.cat, MUSIC, &landing));
    let keys: HashMap<String, String> = lib.pids().unwrap().into_iter().map(|(pid, song)| (song, pid)).collect();
    assert_ne!(keys[&pier_song], keys[&undertow_song], "the two untagged files' keys apart: their folders' albums and places");
    // The rows, written before.
    let ud = UserData::open(None).unwrap();
    for song in [&pier_song, &undertow_song, &landing_song] {
        ud.count_play("alice", (song, &keys[song]), &crate::serve::ids::record(r301), &[], 1_700_000_000).unwrap();
        ud.set_starred("alice", Item::Song, std::slice::from_ref(song), true).unwrap();
        ud.set_rating("alice", Item::Song, song, 4).unwrap();
        ud.set_bookmark("alice", song, Some(&keys[song]), 61_000, "", None).unwrap();
    }
    let lines: Vec<String> = [&pier, &undertow].iter().map(|l| l.to_string()).collect();
    let tracks: HashMap<String, Option<String>> = lines.iter().cloned().zip([Some(pier_song.clone()), Some(undertow_song.clone())]).collect();
    ud.create_playlist("bob", "Late", &lines, &tracks).unwrap();
    let carol = AppQueue { client: "app".into(), songs: vec![pier_song.clone(), undertow_song.clone()], current: Some(0), position_ms: 0, changed: 1 };
    ud.save_app_queue("carol", &carol).unwrap();
    ud.carry_pids(&lib.pids().unwrap()).unwrap();
    // A handover (T-COU-6): Landing's file replaced by a better copy of it, tagged otherwise; the rows on its track carry its key now.
    let better = format!("{TWO_PIERS}/02 Landing.mp3");
    f.cat.place_file(MUSIC, &better, r301, f.cat.track_of(MUSIC, &landing), (1, 2)).unwrap();
    std::fs::remove_file(f.roots[0].join(&landing)).unwrap();
    f.add(MUSIC, &[(better.as_str(), vec![("album", "Two Piers"), ("albumartist", "Glass Harbour"), ("title", "Landing (Remastered)"), ("tracknumber", "2")])]);
    f.pass();
    let lib = built(&f.cat);
    assert_eq!(song_of(&f.cat, MUSIC, &better), landing_song, "the better copy took Landing's track");
    ud.carry_pids(&lib.pids().unwrap()).unwrap();
    let landing_key = lib.pids().unwrap().into_iter().find(|(_, s)| *s == landing_song).map(|(p, _)| p).unwrap();
    // The track ids started afresh: a catalogue made anew over the same files, its ids numbered above the old ones.
    let fresh = Catalogue::open(None, f.roots.clone()).unwrap();
    for _ in 0..50 {
        fresh.db_for_tests().execute("INSERT INTO tracks (born, retired_at) VALUES (1, 1)", []).unwrap();
    }
    read_afresh(&fresh, &better, &[("album", "Two Piers"), ("albumartist", "Glass Harbour"), ("title", "Landing (Remastered)"), ("tracknumber", "2")]);
    let lib = built(&fresh);
    let (new_pier, new_undertow, new_landing) = (song_of(&fresh, MUSIC, &pier), song_of(&fresh, IN, &undertow), song_of(&fresh, MUSIC, &better));
    assert!(![&new_pier, &new_undertow, &new_landing].contains(&&pier_song), "new ids");
    let fresh_keys: HashMap<String, String> = lib.pids().unwrap().into_iter().map(|(pid, song)| (song, pid)).collect();
    assert_eq!(fresh_keys[&new_pier], keys[&pier_song], "Pier's key worked out again the same way");
    assert_eq!(fresh_keys[&new_landing], landing_key, "Landing's too, as handed over");
    let songs: HashSet<String> = lib.all_song_ids().unwrap().into_iter().collect();
    let followed = ud.follow_pids(&lib.pids().unwrap(), &songs).unwrap();
    assert_eq!(followed.apart, 0, "no key two songs share here");
    for (old, new) in [(&pier_song, &new_pier), (&undertow_song, &new_undertow), (&landing_song, &new_landing)] {
        let a = ud.get("alice", Item::Song, std::slice::from_ref(new)).unwrap().remove(new.as_str()).unwrap_or_else(|| panic!("{old}'s annotation followed to {new}"));
        assert_eq!((a.play_count, a.rating), (1, 4), "{old}: plays and rating follow");
        assert!(a.starred_at.is_some(), "{old}: the heart follows");
        assert!(ud.bookmarks("alice").unwrap().iter().any(|b| &b.item == new), "{old}: the bookmark follows");
        assert!(ud.heard_since("alice", 0).unwrap().iter().any(|l| &l.song == new), "{old}: the play history follows");
        assert!(!ud.heard_since("alice", 0).unwrap().iter().any(|l| &l.song == old), "{old}: none left on the old id");
    }
    assert_eq!(ud.playlist_tracks("bob", "Late").unwrap().unwrap(), [Some(new_pier.clone()), Some(new_undertow.clone())], "bob's lines follow");
    assert_eq!(ud.app_queues("carol").unwrap()[0].songs, carol.songs, "carol's saved queue names the old ids, read as missing");
}
