//! SCENARIOS-COUPLING's catalogue half (DC1a part 3b): a file Cratarr placed has no say in its folder's album and is no source of its
//! record's facts (COU.1, COU.2, COU.3, T-COU-1), the record's facts its files' by majority (T-COU-3, T-COU-12), namesakes through the
//! record (COU.4), one order from the track's place (COU.5, COU.6, COU.19), a missing track fetched back (COU.8), what "as placed"
//! means (COU.18). On real folders, as a pass reads them; the served half through a build.

use rusqlite::params;

use super::wm_tests::{Folders, IN, MUSIC, keep, list};
use crate::serve::ids;

const GH1: &str = "6c3a0b22-0000-4000-8000-0000000000a1";
const R1: &str = "00000000-0000-4000-8000-0000000000e1";
const R9: &str = "00000000-0000-4000-8000-0000000000e9";
const R301: &str = "00000000-0000-4000-8000-000000000301";
const R302: &str = "00000000-0000-4000-8000-000000000302";
const R303: &str = "00000000-0000-4000-8000-000000000303";
const CED3: &str = "ced3fdc3-0000-4000-8000-000000000222";
const C18F: &str = "c18f3ccc-0000-4000-8000-000000000221";

/// A file of an album as tagged: its own fields over the album's.
fn tagged<'a>(album: &[(&'a str, &'a str)], own: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    album.iter().chain(own).copied().collect()
}

fn tracks_of(f: &Folders, record: i64) -> Vec<(i64, Option<u32>, Option<u32>)> {
    f.cat
        .db()
        .prepare("SELECT id, disc, number FROM tracks WHERE record = ?1 AND retired_at IS NULL ORDER BY id")
        .unwrap()
        .query_map([record], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn place_of(f: &Folders, folder: i64, rel: &str) -> (Option<u32>, Option<u32>) {
    let track = f.cat.track_of(folder, rel).unwrap();
    f.cat.db().query_row("SELECT disc, number FROM tracks WHERE id = ?1", [track], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
}

/// The file gone outside Cratarr and its track let go (LS5's Gone): its rows gone, its track retired with its plays kept.
fn let_go(f: &Folders, folder: i64, rel: &str) -> i64 {
    let track = f.cat.track_of(folder, rel).unwrap();
    std::fs::remove_file(f.roots[usize::try_from(folder).unwrap()].join(rel)).unwrap();
    let db = f.cat.db();
    db.execute("DELETE FROM files WHERE folder = ?1 AND rel = ?2", params![folder, rel]).unwrap();
    db.execute("DELETE FROM tags WHERE folder = ?1 AND rel = ?2", params![folder, rel]).unwrap();
    db.execute("UPDATE tracks SET retired_at = 1 WHERE id = ?1", [track]).unwrap();
    track
}

/// A file changed outside Cratarr, so a walk finds its size and time changed and its tags are read again as `tags` (COU.18 (b)).
fn retagged(f: &Folders, folder: i64, rel: &str, tags: &[(&str, &str)]) {
    f.cat.db().execute("UPDATE files SET size = size + 7, mtime = mtime + 7 WHERE folder = ?1 AND rel = ?2", params![folder, rel]).unwrap();
    f.cat.tag_for_tests(folder, rel, tags);
}

const TWO_PIERS: &str = "Glass Harbour/Two Piers (2018)";

/// "Two Piers" `al-301` in `folder`: `01 Pier.flac` and `02 Landing.flac`, tagged as its own, its list kept. Its record, track 1.
fn two_piers(f: &Folders, folder: i64) -> (i64, String) {
    let album = [("album", "Two Piers"), ("albumartist", "Glass Harbour"), ("musicbrainz_albumartistid", GH1), ("date", "2018"), ("musicbrainz_albumid", R301)];
    let (pier, landing) = (format!("{TWO_PIERS}/01 Pier.flac"), format!("{TWO_PIERS}/02 Landing.flac"));
    f.add(folder, &[(pier.as_str(), tagged(&album, &[("title", "Pier"), ("tracknumber", "1")])), (landing.as_str(), tagged(&album, &[("title", "Landing"), ("tracknumber", "2")]))]);
    f.lengths(folder, &[(pier.as_str(), 250_000), (landing.as_str(), 240_000)]);
    f.pass();
    let r = f.record_of(folder, &pier).unwrap();
    keep(&f.cat, r, "R301-LIST", &list(&[("Pier", 250_000), ("Landing", 240_000)]));
    (r, pier)
}

/// The folder's albums as a build reads them, in `folder`.
fn albums_in(f: &Folders, folder: i64) -> usize {
    f.albums().iter().filter(|a| a.folder == folder).count()
}

/// COU.1 (a), (b), (d) on WM.23's untouched placing (T-COU-1): a fetched track placed into "Two Piers" (its file as fetched: a promo's
/// ALBUM, "Glass Harbor", 2019; or no tags at all; or in IN) has no vote in its folder's album: one copy, one record, named and
/// credited as before, the new file its track 1; no artist born of its tags. Red first: the folder's two files tie and the placed one,
/// first by path, named the album: the record renamed "Two Piers (Promo)" and credited to a new "Glass Harbor".
#[test]
fn a_fetched_track_has_no_say_in_its_folders_album() {
    let promo = vec![("album", "Two Piers (Promo)"), ("albumartist", "Glass Harbor"), ("date", "2019"), ("tracknumber", "1"), ("title", "Pier")];
    for (case, folder, tags) in [("a", MUSIC, promo.clone()), ("b", MUSIC, Vec::new()), ("d", IN, promo)] {
        let f = Folders::new(&format!("cou1{case}"));
        let (r, pier) = two_piers(&f, folder);
        let credited = f.cat.record_artists(r).unwrap();
        let track = let_go(&f, folder, &pier);
        f.pass();
        f.cat.place_file(folder, &pier, r, Some(track), (1, 1)).unwrap();
        f.add(folder, &[(pier.as_str(), tags.clone())]);
        f.lengths(folder, &[(pier.as_str(), 250_400)]);
        f.pass();
        assert_eq!(albums_in(&f, folder), 1, "({case}) one album of the folder");
        assert_eq!((f.cat.track_of(folder, &pier), f.record_of(folder, &pier)), (Some(track), Some(r)), "({case}) its track 1, its record");
        let row = f.cat.record(r).unwrap().unwrap();
        assert_eq!((row.title.as_str(), row.artist.as_str()), ("Two Piers", "Glass Harbour"), "({case}) named as before");
        assert_eq!(f.cat.record_artists(r).unwrap(), credited, "({case}) credited as before");
        let born: i64 = f.cat.db().query_row("SELECT count(*) FROM artists WHERE key = ?1", [ids::artist_key("Glass Harbor")], |r| r.get(0)).unwrap();
        assert_eq!(born, 0, "({case}) no artist born of its tags");
    }
}

/// COU.1 (c)'s catalogue half (the fetch's placing beside its disc is DC2's): "Long Water" over `CD1/` and `CD2/`, disc 2's "Tide"
/// placed in `CD2/` beside `01 Intro.flac`, its file a peer's single ("Tide", 2020): one copy across both disc folders, four tracks,
/// "Tide" disc 2 track 2; it ties Intro one to one and names nothing. Red first: the placed file named `CD2/`'s album by path.
#[test]
fn a_fetched_track_lies_beside_its_disc() {
    let f = Folders::new("cou1c");
    let dir = "Glass Harbour/Long Water (2017)";
    let album = [("album", "Long Water"), ("albumartist", "Glass Harbour"), ("date", "2017")];
    let files = [("CD1/01 Intro.flac", "1", "1", "Intro"), ("CD1/02 Harbour.flac", "1", "2", "Harbour"), ("CD2/01 Intro.flac", "2", "1", "Intro")];
    let rels: Vec<String> = files.iter().map(|(rel, ..)| format!("{dir}/{rel}")).collect();
    for (rel, (_, disc, n, title)) in rels.iter().zip(files) {
        f.add(MUSIC, &[(rel.as_str(), tagged(&album, &[("discnumber", disc), ("tracknumber", n), ("title", title)]))]);
    }
    f.pass();
    let r = f.record_of(MUSIC, &rels[0]).unwrap();
    let tide = format!("{dir}/CD2/02 Tide.flac");
    f.cat.place_file(MUSIC, &tide, r, None, (2, 2)).unwrap();
    f.add(MUSIC, &[(tide.as_str(), vec![("album", "Tide"), ("albumartist", "Glass Harbour"), ("date", "2020"), ("title", "Tide")])]);
    f.pass();
    assert_eq!(albums_in(&f, MUSIC), 1, "one copy across both disc folders");
    assert_eq!(f.record_of(MUSIC, &tide), Some(r));
    assert_eq!(tracks_of(&f, r).len(), 4, "four tracks in one copy");
    assert_eq!(place_of(&f, MUSIC, &tide), (Some(2), Some(2)), "disc 2 track 2");
    assert_eq!(f.cat.record(r).unwrap().unwrap().title, "Long Water");
}

/// COU.2: a better copy of the one-track single "Pier" `al-302`, its FLAC tagged as the album "Pier Songs" `al-303` (R303, track 1),
/// placed alone in the single's folder: it takes `al-302`'s track, the single stays its own record named "Pier", "Pier Songs" keeps its
/// three tracks in one copy. Red first: in MUSIC the hash has no folder in front, so R303 gathered the FLAC into `al-303`'s album and
/// the pass moved the single's track there.
#[test]
fn a_better_copy_of_a_one_track_record_stays_its_own() {
    let f = Folders::new("cou2");
    let single = "Glass Harbour/Pier (2018)/01 Pier.mp3";
    f.add(MUSIC, &[(single, vec![("album", "Pier"), ("albumartist", "Glass Harbour"), ("title", "Pier"), ("tracknumber", "1"), ("date", "2018"), ("musicbrainz_albumid", R302)])]);
    let songs: Vec<String> = ["Pier", "Stone", "Gull"].iter().enumerate().map(|(i, t)| format!("Glass Harbour/Pier Songs (2019)/0{} {t}.flac", i + 1)).collect();
    for (i, rel) in songs.iter().enumerate() {
        let n = (i + 1).to_string();
        let title = ["Pier", "Stone", "Gull"][i];
        f.add(MUSIC, &[(rel.as_str(), vec![("album", "Pier Songs"), ("albumartist", "Glass Harbour"), ("title", title), ("tracknumber", n.as_str()), ("date", "2019"), ("musicbrainz_albumid", R303)])]);
    }
    f.pass();
    let (r302, r303) = (f.record_of(MUSIC, single).unwrap(), f.record_of(MUSIC, &songs[0]).unwrap());
    let track = f.cat.track_of(MUSIC, single).unwrap();
    let better = "Glass Harbour/Pier (2018)/01 Pier.flac";
    f.cat.place_file(MUSIC, better, r302, Some(track), (1, 1)).unwrap();
    std::fs::remove_file(f.roots[0].join(single)).unwrap();
    f.add(MUSIC, &[(better, vec![("album", "Pier Songs"), ("albumartist", "Glass Harbour"), ("title", "Pier"), ("tracknumber", "1"), ("date", "2019"), ("musicbrainz_albumid", R303)])]);
    f.pass();
    assert_eq!((f.cat.track_of(MUSIC, better), f.record_of(MUSIC, better)), (Some(track), Some(r302)), "the single's track, its record");
    assert_eq!(f.cat.record(r302).unwrap().unwrap().title, "Pier", "the single stays itself");
    assert_eq!(tracks_of(&f, r303).len(), 3, "Pier Songs keeps its three tracks");
    assert_eq!(f.albums().iter().filter(|a| a.tracks.len() == 3).count(), 1, "Pier Songs' album gathers no file of another folder");
}

const NIGHT_FERRY: &str = "Glass Harbour/Night Ferry (2014)";

/// "Night Ferry": N1-N10 numbered 1-10, every file ALBUM "Night Ferry", ALBUMARTIST "Glass Harbour" with `gh-1`, DATE 2014, LABEL
/// "Harbour Tapes", R1. Its record and its files.
fn night_ferry(f: &Folders, folder: i64) -> (i64, Vec<String>) {
    let album = [("album", "Night Ferry"), ("albumartist", "Glass Harbour"), ("musicbrainz_albumartistid", GH1), ("date", "2014"), ("label", "Harbour Tapes"), ("musicbrainz_albumid", R1)];
    let rels: Vec<String> = (1..=10).map(|n| format!("{NIGHT_FERRY}/{n:02} N{n}.flac")).collect();
    for (n, rel) in (1..=10).zip(&rels) {
        let (title, number) = (format!("N{n}"), n.to_string());
        f.add(folder, &[(rel.as_str(), tagged(&album, &[("title", title.as_str()), ("tracknumber", number.as_str())]))]);
    }
    f.lengths(folder, &rels.iter().zip(1..).map(|(rel, n)| (rel.as_str(), 180_000 + n * 7_000)).collect::<Vec<_>>());
    f.pass();
    (f.record_of(folder, &rels[0]).unwrap(), rels)
}

/// The reissue's tags a fetched N1 came with (COU.3's Given).
const REISSUE: [(&str, &str); 7] = [
    ("tracknumber", "1"),
    ("album", "Night Ferry (Remastered)"),
    ("albumartist", "Glass Harbour"),
    ("date", "2021"),
    ("label", "Reissue Records"),
    ("musicbrainz_albumid", R9),
    ("title", "N1"),
];

/// The served album of `record`: (name, year, MusicBrainz release, its album artist's MusicBrainz id).
fn served_album(f: &Folders, record: i64) -> (String, Option<u32>, Option<String>, Option<String>) {
    let lib = crate::serve::library::Library::open(None).unwrap();
    crate::serve::library::testing::build_now(&lib, &f.cat);
    let db = lib.db();
    db.query_row(
        "SELECT al.name, al.year, al.mbid, (SELECT mbid FROM album_artists aa WHERE aa.album_id = al.id AND aa.position = 0) FROM albums al WHERE al.id = ?1",
        [ids::record(record)],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .unwrap()
}

fn labels_of(f: &Folders, record: i64) -> Vec<String> {
    f.cat
        .db()
        .prepare("SELECT l.name FROM record_labels r JOIN labels l ON l.id = r.label WHERE r.record = ?1 ORDER BY l.name")
        .unwrap()
        .query_map([record], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn releases_of(f: &Folders, record: i64) -> Vec<String> {
    f.cat
        .db()
        .prepare("SELECT value FROM \"references\" WHERE subject = ?1 AND source = 'musicbrainz' AND kind = 'release' ORDER BY value")
        .unwrap()
        .query_map([ids::record(record)], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// COU.3 (b) (T-COU-12, T-COU-3): a record fact with no hand value is its deciding copy's files' by majority, a tie to the first in
/// track order: N1 retagged outside Cratarr as a 2021 reissue (another album, label and release), the other nine as they were: nine
/// to one keep "Night Ferry", 2014, "Harbour Tapes", R1 and `gh-1`, in the catalogue and on the served album. Red first: the latest
/// date, the first file's label and release won over nine files.
#[test]
fn a_records_facts_are_its_files_majority() {
    let f = Folders::new("cou3b");
    let (r, rels) = night_ferry(&f, MUSIC);
    let credited = f.cat.record_artists(r).unwrap();
    retagged(&f, MUSIC, &rels[0], &REISSUE);
    f.pass();
    assert_eq!(f.cat.record(r).unwrap().unwrap().title, "Night Ferry");
    assert_eq!(f.cat.record_artists(r).unwrap(), credited, "credited by gh-1, as before");
    assert_eq!(labels_of(&f, r), ["Harbour Tapes"], "the labels most files name");
    assert_eq!(releases_of(&f, r), [R1], "the release most files name");
    assert_eq!(served_album(&f, r), ("Night Ferry".into(), Some(2014), Some(R1.into()), Some(GH1.into())), "every surface: the stored facts");
}

/// COU.3 (a), (c) on WM.23's untouched placing (T-COU-1): N1 let go and fetched back with the reissue's tags, placed and as placed:
/// "Night Ferry", credited by `gh-1`, 2014, R1 and "Harbour Tapes", each as before; the placed file is N1. Red first (on the placing):
/// the first file, the placed one, gave the release, label and year.
#[test]
fn a_placed_file_is_no_source_of_its_records_facts() {
    let f = Folders::new("cou3a");
    let (r, rels) = night_ferry(&f, MUSIC);
    let credited = f.cat.record_artists(r).unwrap();
    let n1 = let_go(&f, MUSIC, &rels[0]);
    f.pass();
    f.cat.place_file(MUSIC, &rels[0], r, Some(n1), (1, 1)).unwrap();
    f.add(MUSIC, &[(rels[0].as_str(), REISSUE.to_vec())]);
    f.pass();
    assert_eq!(f.cat.track_of(MUSIC, &rels[0]), Some(n1), "the placed file is N1");
    assert_eq!(f.cat.record(r).unwrap().unwrap().title, "Night Ferry");
    assert_eq!(f.cat.record_artists(r).unwrap(), credited);
    assert_eq!(labels_of(&f, r), ["Harbour Tapes"]);
    assert_eq!(releases_of(&f, r), [R1], "never the reissue's reference");
    assert_eq!(served_album(&f, r), ("Night Ferry".into(), Some(2014), Some(R1.into()), Some(GH1.into())));
}

/// COU.18 (T-COU-17): what is read and when. (a) N1 retagged keeping its size and time: nothing is read, it stays as placed, at its
/// told place; (b) retagged with its time changed: read at the next pass, its tags differing from those read at placing, so it is no
/// longer as placed and speaks as one of the copy's files (COU.3 (b)); (c) "Read again" after (a): read, then as (b). Red first: no
/// "as placed" held its tags (a read of changed tags left the file as placed).
#[test]
fn a_placed_file_speaks_once_read_otherwise() {
    let f = Folders::new("cou18");
    let (r, rels) = night_ferry(&f, MUSIC);
    let n1 = let_go(&f, MUSIC, &rels[0]);
    f.pass();
    f.cat.place_file(MUSIC, &rels[0], r, Some(n1), (1, 1)).unwrap();
    f.add(MUSIC, &[(rels[0].as_str(), vec![("title", "N1"), ("tracknumber", "7")])]);
    f.pass();
    let told = |f: &Folders| f.cat.served_rows(MUSIC).unwrap().into_iter().find(|row| row.rel == rels[0]).unwrap().told;
    assert_eq!(told(&f), Some((1, 1)), "(a) as placed: nothing read");
    assert_eq!(place_of(&f, MUSIC, &rels[0]), (Some(1), Some(1)), "at its told place, never its own 7");
    // (c) Read again: its tags read (they differ from those read at placing): no longer as placed.
    f.cat.tag_for_tests(MUSIC, &rels[0], &[("title", "N1"), ("tracknumber", "1"), ("album", "Night Ferry")]);
    assert_eq!(told(&f), None, "(c) read again, its tags changed: it speaks");
    // (b) a time changed: read at the next pass, as (c).
    let g = Folders::new("cou18b");
    let (r, rels) = night_ferry(&g, MUSIC);
    let n1 = let_go(&g, MUSIC, &rels[0]);
    g.pass();
    g.cat.place_file(MUSIC, &rels[0], r, Some(n1), (1, 1)).unwrap();
    g.add(MUSIC, &[(rels[0].as_str(), vec![("title", "N1")])]);
    g.pass();
    assert_eq!(told(&g), Some((1, 1)));
    retagged(&g, MUSIC, &rels[0], &REISSUE);
    assert_eq!(told(&g), None, "(b) no longer as placed");
    g.pass();
    assert_eq!(g.cat.record(r).unwrap().unwrap().title, "Night Ferry", "(b) one of ten files, outvoted");
}

/// COU.18 (d)'s catalogue half (kind 4) and COU.8: a placement told before its file appears (a pass between the rename and the telling
/// is the requests crate's): the pass leaves the file to its placement, which takes the track it was fetched for: (a) a track let go
/// (retired) comes back, (b) a track gone and waiting. Never a fresh id, never found by its tags or its name alone. Green on part 3a's
/// `place_file` (its handover); kept as the pin of COU.8.
#[test]
fn a_missing_track_fetched_comes_back_as_itself() {
    for retired in [true, false] {
        let f = Folders::new(if retired { "cou8a" } else { "cou8b" });
        let (r, pier) = two_piers(&f, MUSIC);
        let track = if retired {
            let_go(&f, MUSIC, &pier)
        } else {
            let t = f.cat.track_of(MUSIC, &pier).unwrap();
            std::fs::remove_file(f.roots[0].join(&pier)).unwrap();
            f.cat.walk_all();
            t
        };
        f.pass();
        f.cat.place_file(MUSIC, &pier, r, Some(track), (1, 1)).unwrap();
        f.add(MUSIC, &[(pier.as_str(), vec![("title", "Pier (Remastered 2011)")])]);
        f.lengths(MUSIC, &[(pier.as_str(), 250_300)]);
        f.pass();
        assert_eq!(f.cat.track_of(MUSIC, &pier), Some(track), "retired {retired}: the track it was fetched for");
        let back: Option<i64> = f.cat.db().query_row("SELECT retired_at FROM tracks WHERE id = ?1", [track], |r| r.get(0)).unwrap();
        assert_eq!((back, f.record_of(MUSIC, &pier)), (None, Some(r)), "retired {retired}: back, on its record");
    }
}

/// COU.4's catalogue half (T-COU-2, T-COU-3): "Allez" credited to the second Salamanda by its files' id. (b) a bonus file dropped by
/// hand, no ids, sorting first: the credit stays; a file sorting first naming the other Salamanda's id, the rest this one's: by
/// majority, never the first file's alone; (d) every file retagged to the other's id: re-credited by evidence. Red first: the credit's
/// ids were the first file's alone.
#[test]
fn namesakes_stay_apart_through_the_record() {
    let f = Folders::new("cou4");
    let first = f.cat.artist_for("Salamanda").unwrap();
    f.cat.set_reference(&ids::artist(first), &super::records::Ref::new("musicbrainz", "artist", C18F), "files").unwrap();
    let album = [("album", "Allez"), ("albumartist", "Salamanda"), ("artist", "Salamanda"), ("musicbrainz_albumartistid", CED3)];
    let rels: Vec<String> = (1..=4).map(|n| format!("Salamanda/Allez/0{n} A{n}.flac")).collect();
    for (n, rel) in (1..=4).zip(&rels) {
        let (title, number) = (format!("A{n}"), n.to_string());
        f.add(MUSIC, &[(rel.as_str(), tagged(&album, &[("title", title.as_str()), ("tracknumber", number.as_str())]))]);
    }
    f.pass();
    let r = f.record_of(MUSIC, &rels[0]).unwrap();
    let second = f.cat.record_artists(r).unwrap();
    assert_eq!(second.len(), 1);
    assert_ne!(second[0], first, "the second Salamanda, by its id");
    // (b) a bonus dropped by hand, no ids, "1": sorts first.
    f.add(MUSIC, &[("Salamanda/Allez/00 Intro.flac", vec![("album", "Allez"), ("albumartist", "Salamanda"), ("artist", "Salamanda"), ("title", "Intro"), ("tracknumber", "1")])]);
    f.pass();
    assert_eq!(f.cat.record_artists(r).unwrap(), second, "(b) the absence of an id is no evidence");
    // T-COU-3: the first file names the other Salamanda's id; three name this one's.
    retagged(&f, MUSIC, "Salamanda/Allez/00 Intro.flac", &[("album", "Allez"), ("albumartist", "Salamanda"), ("title", "Intro"), ("tracknumber", "1"), ("musicbrainz_albumartistid", C18F)]);
    f.pass();
    assert_eq!(f.cat.record_artists(r).unwrap(), second, "by majority, never the first file's alone");
    // (d) every file retagged to the other's id.
    for rel in rels.iter().map(String::as_str).chain(["Salamanda/Allez/00 Intro.flac"]) {
        let title = rel.rsplit_once(' ').map_or("", |(_, t)| t.trim_end_matches(".flac"));
        retagged(&f, MUSIC, rel, &[("album", "Allez"), ("albumartist", "Salamanda"), ("title", title), ("musicbrainz_albumartistid", C18F)]);
    }
    f.pass();
    assert_eq!(f.cat.record_artists(r).unwrap(), [first], "(d) re-credited by the files' id, by majority");
}

/// COU.5 (a)-(c) (T-COU-4, T-COU-11): a placed file sits at its place: N3 let go and fetched as a compilation's disc 2 track 7, or
/// with no numbers; N5 replaced by a better copy numbered 12: N3 at disc 1 track 3, N5 at 5, the served song's `track` and
/// `discNumber` their places, one order on every surface. Red first: the served numbers and the order read the placed file's tags.
#[test]
fn a_placed_file_sits_at_its_place_everywhere() {
    for (case, tags) in [("a", vec![("title", "N3"), ("tracknumber", "7"), ("discnumber", "2")]), ("b", vec![("title", "N3")])] {
        let f = Folders::new(&format!("cou5{case}"));
        let (r, rels) = night_ferry(&f, MUSIC);
        let n3 = let_go(&f, MUSIC, &rels[2]);
        f.pass();
        f.cat.place_file(MUSIC, &rels[2], r, Some(n3), (1, 3)).unwrap();
        f.add(MUSIC, &[(rels[2].as_str(), tags)]);
        f.pass();
        assert_eq!(place_of(&f, MUSIC, &rels[2]), (Some(1), Some(3)), "({case}) disc 1 track 3");
        let lib = crate::serve::library::Library::open(None).unwrap();
        crate::serve::library::testing::build_now(&lib, &f.cat);
        let order: Vec<(String, Option<u32>, Option<u32>)> = lib
            .db()
            .prepare("SELECT title, disc_no, track_no FROM songs WHERE album_id = ?1 ORDER BY coalesce(disc_no, 1), track_no IS NULL, track_no, rel")
            .unwrap()
            .query_map([ids::record(r)], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(order[2], ("N3".to_string(), Some(1), Some(3)), "({case}) the served song at its place: {order:?}");
        assert_eq!(order.iter().map(|(t, ..)| t.as_str()).collect::<Vec<_>>(), (1..=10).map(|n| format!("N{n}")).collect::<Vec<_>>(), "({case}) one order");
    }
    // (c) N5 replaced by a better copy numbered 12.
    let f = Folders::new("cou5c");
    let (r, rels) = night_ferry(&f, MUSIC);
    let n5 = f.cat.track_of(MUSIC, &rels[4]).unwrap();
    let better = format!("{NIGHT_FERRY}/05 N5.mp3");
    f.cat.place_file(MUSIC, &better, r, Some(n5), (1, 5)).unwrap();
    std::fs::remove_file(f.roots[0].join(&rels[4])).unwrap();
    f.add(MUSIC, &[(better.as_str(), vec![("title", "N5"), ("tracknumber", "12")])]);
    f.pass();
    assert_eq!((f.cat.track_of(MUSIC, &better), place_of(&f, MUSIC, &better)), (Some(n5), (Some(1), Some(5))), "(c) N5 at 5");
}

/// COU.6 (OQ-COU-1 (a)): your own file's number stands until a list is picked. (a) N4 and N5 both say 4 by a typo, no list picked:
/// both at 4; (c) MusicBrainz's list picked: the pairing orders, N5 at 5; after Reset, (a)'s; (d) the typo fixed outside Cratarr: read
/// at the next scan, N5 at 5. Red first: (c) a pick ordered nothing.
#[test]
fn a_picked_list_orders_the_record() {
    let f = Folders::new("cou6");
    let (r, rels) = night_ferry(&f, MUSIC);
    let album = [("album", "Night Ferry"), ("albumartist", "Glass Harbour"), ("musicbrainz_albumartistid", GH1), ("date", "2014"), ("label", "Harbour Tapes"), ("musicbrainz_albumid", R1)];
    retagged(&f, MUSIC, &rels[4], &tagged(&album, &[("title", "N5"), ("tracknumber", "4")]));
    f.pass();
    assert_eq!((place_of(&f, MUSIC, &rels[3]), place_of(&f, MUSIC, &rels[4])), ((Some(1), Some(4)), (Some(1), Some(4))), "(a) your files' numbers stand");
    let titles: Vec<(String, i64)> = (1..=10).map(|n| (format!("N{n}"), 180_000 + n * 7_000)).collect();
    let mb = list(&titles.iter().map(|(t, ms)| (t.as_str(), *ms)).collect::<Vec<_>>());
    keep(&f.cat, r, "R1-LIST", &mb);
    let picked = f.cat.lists_kept(r).unwrap().into_iter().find(|l| l.reference.value == "R1-LIST").unwrap().reference;
    f.cat.pick_list(r, Some(&picked)).unwrap();
    f.pass();
    assert_eq!(place_of(&f, MUSIC, &rels[4]), (Some(1), Some(5)), "(c) the pick orders");
    f.cat.pick_list(r, None).unwrap();
    f.pass();
    assert_eq!(place_of(&f, MUSIC, &rels[4]), (Some(1), Some(4)), "(c) after Reset, (a)'s");
    retagged(&f, MUSIC, &rels[4], &tagged(&album, &[("title", "N5"), ("tracknumber", "5")]));
    f.pass();
    assert_eq!(place_of(&f, MUSIC, &rels[4]), (Some(1), Some(5)), "(d) read at the next scan");
}

/// COU.6 (b) (T-COU-10, T-COU-11): "Two Rooms" `al-9`'s Japanese edition numbers a bonus Intro 1, so its list's eight are the copy's
/// 2-9; the list's track 4 (the copy's 5) fetched, as fetched numbered 4: it sits between the files that pair with the list's 3 and 5,
/// the copy's 5. Red first: the place given its list's number, 4.
#[test]
fn a_placed_file_takes_its_copys_number() {
    let f = Folders::new("cou6b");
    let dir = "Kite Theory/Two Rooms";
    let album = [("album", "Two Rooms"), ("albumartist", "Kite Theory")];
    let list8: Vec<(String, i64)> = (1..=8).map(|n| (format!("R{n}"), 170_000 + n * 9_000)).collect();
    let mut files: Vec<(String, String, u32, i64)> = vec![(format!("{dir}/01 Intro.flac"), "Intro".into(), 1, 60_000)];
    files.extend(list8.iter().zip(2..).filter(|(_, n)| *n != 5).map(|((t, ms), n)| (format!("{dir}/{n:02} {t}.flac"), t.clone(), n, *ms)));
    for (rel, title, n, _) in &files {
        let number = n.to_string();
        f.add(MUSIC, &[(rel.as_str(), tagged(&album, &[("title", title.as_str()), ("tracknumber", number.as_str())]))]);
    }
    f.lengths(MUSIC, &files.iter().map(|(rel, _, _, ms)| (rel.as_str(), *ms)).collect::<Vec<_>>());
    f.pass();
    let r = f.record_of(MUSIC, &files[0].0).unwrap();
    keep(&f.cat, r, "R9JP", &list(&list8.iter().map(|(t, ms)| (t.as_str(), *ms)).collect::<Vec<_>>()));
    let fetched = format!("{dir}/05 R4.flac");
    f.cat.place_file(MUSIC, &fetched, r, None, (1, 4)).unwrap();
    f.add(MUSIC, &[(fetched.as_str(), vec![("title", "R4"), ("tracknumber", "4")])]);
    f.lengths(MUSIC, &[(fetched.as_str(), list8[3].1)]);
    f.pass();
    assert_eq!(place_of(&f, MUSIC, &fetched), (Some(1), Some(5)), "the copy's 5, between its neighbours on the list");
    assert_eq!(place_of(&f, MUSIC, &files[0].0), (Some(1), Some(1)), "the Intro at 1");
}

/// COU.19, a guard (FA2, D.1): Night Ferry in IN (a reissue numbering every track one on, 2-11, imported first) and MUSIC (1-10), one
/// record on the same tracks: the record's order is MUSIC's, the copy that decides, whichever copy plays; once MUSIC's copy is gone,
/// IN's numbers decide, read at that scan. Red first: the place given once from IN's numbers (a COU.5 build keeping it fails this).
#[test]
fn a_records_order_is_its_deciding_copys() {
    let f = Folders::new("cou19");
    let album = [("album", "Night Ferry"), ("albumartist", "Glass Harbour"), ("musicbrainz_albumartistid", GH1), ("musicbrainz_albumid", R1)];
    let in_rels: Vec<String> = (1..=10).map(|n| format!("{NIGHT_FERRY}/N{n}.flac")).collect();
    for (n, rel) in (1..=10).zip(&in_rels) {
        let (title, number) = (format!("N{n}"), (n + 1).to_string());
        f.add(IN, &[(rel.as_str(), tagged(&album, &[("title", title.as_str()), ("tracknumber", number.as_str())]))]);
    }
    f.lengths(IN, &in_rels.iter().zip(1..).map(|(rel, n)| (rel.as_str(), 180_000 + n * 7_000)).collect::<Vec<_>>());
    f.pass();
    let r = f.record_of(IN, &in_rels[0]).unwrap();
    assert_eq!(place_of(&f, IN, &in_rels[9]), (Some(1), Some(11)), "IN's numbers, alone");
    let (_, rels) = night_ferry(&f, MUSIC);
    let ranked = |f: &Folders| {
        let mut albums = f.albums();
        albums.iter_mut().for_each(|a| a.rank = if a.folder == MUSIC { 2 } else { 0 });
        f.cat.assign_records(&albums).unwrap();
    };
    ranked(&f);
    assert_eq!(f.record_of(MUSIC, &rels[0]), Some(r), "one record, two copies");
    assert_eq!(f.cat.track_of(MUSIC, &rels[9]), f.cat.track_of(IN, &in_rels[9]), "on the same tracks");
    assert_eq!(place_of(&f, IN, &in_rels[9]), (Some(1), Some(10)), "MUSIC's order, the copy that decides, whichever plays");
    // MUSIC's copy deleted (Delete moves its folder to the trash: its rows marked trashed, no copy there any more).
    f.cat.mark_trashed(MUSIC, NIGHT_FERRY, "100").unwrap();
    ranked(&f);
    assert_eq!(place_of(&f, IN, &in_rels[9]), (Some(1), Some(11)), "MUSIC's gone: IN's numbers, read at that scan");
}

/// COU.13 (d) (OQ-COU-2 (a)): a record whose files' ALBUM is a peer's typo, "Night Fery": root sets the title "Night Ferry" in the
/// record's Edit, a hand fact with Reset: the served album and its songs are named by it; after Reset, the files' again; the files'
/// ALBUM stays as it is. Red first: no title fix but a tag.
#[test]
fn a_records_title_by_hand() {
    let f = Folders::new("cou13d");
    let rels: Vec<String> = (1..=2).map(|n| format!("Glass Harbour/Night Fery/0{n} N{n}.flac")).collect();
    for (n, rel) in (1..=2).zip(&rels) {
        let (title, number) = (format!("N{n}"), n.to_string());
        f.add(MUSIC, &[(rel.as_str(), vec![("album", "Night Fery"), ("albumartist", "Glass Harbour"), ("title", title.as_str()), ("tracknumber", number.as_str())])]);
    }
    f.pass();
    let r = f.record_of(MUSIC, &rels[0]).unwrap();
    let album = ids::record(r);
    let named = |set: Option<&str>| -> Vec<String> {
        let lib = crate::serve::library::Library::open(None).unwrap();
        let set = crate::userdata::overrides::Overrides { title: set.map(str::to_string), ..Default::default() };
        lib.set_overrides(std::collections::HashMap::from([(album.clone(), set)]));
        crate::serve::library::testing::build_now(&lib, &f.cat);
        let db = lib.db();
        let name: String = db.query_row("SELECT name FROM albums WHERE id = ?1", [&album], |r| r.get(0)).unwrap();
        let songs: Vec<String> = db.prepare("SELECT album FROM songs WHERE album_id = ?1").unwrap().query_map([&album], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        [vec![name], songs].concat()
    };
    assert_eq!(named(Some("Night Ferry")), ["Night Ferry"; 3], "the hand's title on the album and its songs");
    assert_eq!(named(None), ["Night Fery"; 3], "after Reset, the files'");
    let album_tag: String = f.cat.db().query_row("SELECT value FROM tags WHERE rel = ?1 AND name = 'album'", [&rels[0]], |r| r.get(0)).unwrap();
    assert_eq!(album_tag, "Night Fery", "the files' ALBUM as it is");
    assert_eq!(crate::userdata::overrides::check("title", &serde_json::json!("  Night Ferry ")), Ok(Some(serde_json::json!("Night Ferry"))));
    assert_eq!(crate::userdata::overrides::check("title", &serde_json::Value::Null), Ok(None), "Reset");
}
