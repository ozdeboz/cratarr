//! SCENARIOS-COUPLING through the gate (DC1a part 3c): adding an arrival, and Replace, name the files from the record's facts in the
//! database (COU.15) and tell the copy they place to the catalogue as its record's (COU.17 (c), (d), T-COU-16); Replace by an untagged
//! arrival keeps every id and writes nothing, carrying over refused whole (COU.7, T-COU-5); Keep and Replace take the one lock over the
//! library's files (0092). On real folders: MUSIC (catalogue folder 0, the main library) and IN (1, an Incoming).

use std::path::Path;
use std::sync::Arc;

use cratarr_domain::facts::{RefTrack, Reference};

use super::tests::{folder, lying, on_disk};
use super::*;

const LOW_TIDE: &str = "soulseek/Glass Harbour/Low Tide";
const HIGH_TIDE: &str = "soulseek/Glass Harbour/High Tide";
const MUMIA_DIR: &str = "soulseek/Polo & Pan/Mumia (2023) 2";
const STAMP: &str = "www.electronicfresh.com";

/// MUSIC's template in COU.15: `{albumartist}/{album} ({originalyear})/{track:00} {title}`, two digits.
fn template() -> crate::naming::Template {
    "{albumartist}/{album} ({originalyear})/{track:02} {title}".parse().unwrap()
}

/// A record, made as a list keeps one not had: its title and artist, its list kept under `release` (MusicBrainz, asked), its year
/// MusicBrainz's.
fn record_of_list(cat: &crate::catalogue::Catalogue, title: &str, artist: &str, release: &str, tracks: &[(&str, i64)], year: &str) -> i64 {
    let r = {
        let db = cat.db_for_tests();
        db.execute("INSERT INTO records (born, title, artist) VALUES (1, ?1, ?2)", [title, artist]).unwrap();
        let r = db.last_insert_rowid();
        db.execute(
            "INSERT INTO \"references\" (subject, source, kind, value, at, how) VALUES (?1, 'musicbrainz', 'release', ?2, 1, 'asked')",
            [crate::serve::ids::record(r), release.to_string()],
        )
        .unwrap();
        r
    };
    let list: Vec<RefTrack> = tracks.iter().zip(1..).map(|((t, ms), position)| RefTrack { disc: 1, position, title: t.to_string(), length_ms: Some(*ms), recording: None }).collect();
    cat.keep_list(r, &Reference::new("musicbrainz", "release", release), &list, 1).unwrap();
    cat.said(r, "year", "musicbrainz", &format!("release:{release}"), Some(year), 1).unwrap();
    r
}

/// These files in `folder` under `root`, each (path, its tags, its length), read and identified.
fn files(cat: &crate::catalogue::Catalogue, root: &Path, folder: i64, files: &[(String, Vec<(&str, &str)>, i64)]) {
    for (rel, ..) in files {
        std::fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
        std::fs::write(root.join(rel), rel).unwrap();
    }
    cat.walk_all();
    cat.pretend_read_for_tests();
    for (rel, tags, ms) in files {
        cat.tag_for_tests(folder, rel, tags);
        cat.db_for_tests().execute("UPDATE files SET duration_ms = ?3 WHERE folder = ?1 AND rel = ?2", rusqlite::params![folder, rel, ms]).unwrap();
    }
    cat.identify().unwrap();
}

/// A record pass over every album, as a build gives it.
fn pass(cat: &crate::catalogue::Catalogue) {
    let folders: Vec<crate::serve::library::FolderRows> = cat
        .folders()
        .iter()
        .map(|(folder, root)| crate::serve::library::FolderRows { folder, root: root.display().to_string(), rows: cat.served_rows(folder).unwrap() })
        .collect();
    cat.assign_records(&crate::serve::library::album_copies(&folders, &crate::catalogue::FolderRules::default())).unwrap();
}

fn record_at(cat: &crate::catalogue::Catalogue, folder: i64, rel: &str) -> Option<i64> {
    let track = cat.track_of(folder, rel)?;
    cat.db_for_tests().query_row("SELECT record FROM tracks WHERE id = ?1", [track], |r| r.get(0)).ok().flatten()
}

/// An arrival in IN placed for `record` (a Soulseek placing): its files as Soulseek left them, every one ARTIST "Glass Harbour", no
/// ALBUMARTIST, DATE or TRACKNUMBER, ALBUM `album` (none: no ALBUM at all), named in another order than the list's, each its track's
/// title and length.
fn placed_arrival(cat: &crate::catalogue::Catalogue, inc: &Path, record: i64, dir: &str, tracks: &[(&str, i64)], album: Option<&str>) {
    cat.place_copy(1, dir, record).unwrap();
    let names = ["c", "a", "e", "b", "d", "f"];
    let made: Vec<(String, Vec<(&str, &str)>, i64)> = tracks
        .iter()
        .zip(names)
        .map(|((title, ms), name)| {
            let mut tags = vec![("artist", "Glass Harbour"), ("title", *title)];
            tags.extend(album.map(|a| ("album", a)));
            (format!("{dir}/{name}.flac"), tags, *ms)
        })
        .collect();
    files(cat, inc, 1, &made);
}

const L: [(&str, i64); 5] = [("L1", 200_000), ("L2", 230_000), ("L3", 260_000), ("L4", 290_000), ("L5", 320_000)];
const H: [(&str, i64); 6] = [("H1", 150_000), ("H2", 175_000), ("H3", 200_000), ("H4", 225_000), ("H5", 250_000), ("H6", 275_000)];

/// The arrival at `dir` in IN, listed as Cratarr's own source lists it.
fn located(record: i64, title: &str, dir: &str, inc: &Path) -> Located {
    let album = source::Album { id: crate::serve::ids::record(record), title: title.into(), path: Some(dir.into()), ..Default::default() };
    Located { source: "cratarr".into(), album, dir: IncomingDir { folder: 1, dir: dir.into(), release_group: None }, root: inc.to_path_buf() }
}

fn placed_in(cat: &crate::catalogue::Catalogue, folder: i64, dir: &str) -> Option<i64> {
    cat.db_for_tests().query_row("SELECT record FROM placements WHERE folder = ?1 AND path = ?2", rusqlite::params![folder, dir], |r| r.get(0)).ok()
}

/// COU.15 (a), (f) through the gate (Keep, the move of an arrival into MUSIC), with COU.17 (c)'s telling: "Low Tide" `al-305`, its
/// arrival joined by its item. (a) Added, nothing asked: `Glass Harbour/Low Tide (2016)/01 L1.flac` … `05 L5.flac`, each file's bytes
/// as it arrived; the copy told to the catalogue as `al-305`'s before its files appear. (f) MUSIC already holding a copy of `al-305`
/// in the folder the record's names give: Add says so and offers Replace in; nothing moves. Red first: named from the raw tags
/// (no year, no numbers, the site's ALBUM), and a folder there refused as anyone's.
#[tokio::test]
async fn adding_through_the_gate_names_from_the_record() {
    let (music, inc) = (folder("cou15-gate-music"), folder("cou15-gate-inc"));
    let svc = on_disk(&music, &inc).await;
    svc.lib.set_saved_template(Some(template()));
    let cat = svc.lib.catalogue();
    let r = record_of_list(&cat, "Low Tide", "Glass Harbour", "R305", &L, "2016");
    placed_arrival(&cat, &inc, r, LOW_TIDE, &L, Some("www.flacsite.net"));
    pass(&cat);
    assert_eq!(record_at(&cat, 1, &format!("{LOW_TIDE}/c.flac")), Some(r), "the arrival joined its item's record");
    let arrived = lying(&inc.join(LOW_TIDE));
    let done = svc.move_located("cratarr", located(r, "Low Tide", LOW_TIDE, &inc), 0, crate::settings::Quality::None, None).await.expect("(a) added, nothing asked");
    let placed = "Glass Harbour/Low Tide (2016)";
    assert_eq!(done.folder, placed, "(a) the record's credit, title and year; the stamp names nothing");
    let landed = lying(&music.join(placed));
    let names: Vec<&String> = landed.keys().collect();
    assert_eq!(names, ["01 L1.flac", "02 L2.flac", "03 L3.flac", "04 L4.flac", "05 L5.flac"], "(a) each file its pairing's number and its track's title");
    // c.flac is L1, a.flac L2, e.flac L3, b.flac L4, d.flac L5: each file's bytes as it arrived.
    for (name, from) in [("01 L1.flac", "c.flac"), ("02 L2.flac", "a.flac"), ("03 L3.flac", "e.flac"), ("04 L4.flac", "b.flac"), ("05 L5.flac", "d.flac")] {
        assert_eq!(landed[name].0, arrived[from].0, "(a) {name}'s bytes as {from} arrived: the move and its names are no tag write");
    }
    assert_eq!(placed_in(&cat, 0, placed), Some(r), "(a) the copy it placed told to the catalogue as its record's");
    for d in [music, inc] {
        std::fs::remove_dir_all(d).ok();
    }
    // (f) MUSIC holds a copy of al-305 already, an MP3 copy where the record's names put the arrival.
    let (music, inc) = (folder("cou15f-gate-music"), folder("cou15f-gate-inc"));
    let svc = on_disk(&music, &inc).await;
    svc.lib.set_saved_template(Some(template()));
    let cat = svc.lib.catalogue();
    let r = record_of_list(&cat, "Low Tide", "Glass Harbour", "R305", &L, "2016");
    let held: Vec<(String, Vec<(&str, &str)>, i64)> = L
        .iter()
        .zip(1..)
        .map(|((t, ms), n)| {
            let number: &'static str = ["1", "2", "3", "4", "5"][n - 1];
            (format!("{placed}/0{n} {t}.mp3"), vec![("album", "Low Tide"), ("albumartist", "Glass Harbour"), ("title", *t), ("tracknumber", number), ("date", "2016")], *ms)
        })
        .collect();
    cat.place_copy(0, placed, r).unwrap();
    files(&cat, &music, 0, &held);
    placed_arrival(&cat, &inc, r, LOW_TIDE, &L, Some("www.flacsite.net"));
    pass(&cat);
    let before = (lying(&music), lying(&inc));
    let (_, plan) = svc.plan_keep(&located(r, "Low Tide", LOW_TIDE, &inc), 0, None).await.unwrap();
    match plan {
        Err(KeepRefused::Other(why)) => assert!(why.contains("already holds a copy of it") && why.contains("Replace in"), "(f) says so, offers Replace in: {why}"),
        other => panic!("(f) refused: two copies never share a folder: {other:?}"),
    }
    let tapped = svc.move_located("cratarr", located(r, "Low Tide", LOW_TIDE, &inc), 0, crate::settings::Quality::None, None).await;
    assert!(tapped.is_err(), "(f) the Add refused");
    assert_eq!((lying(&music), lying(&inc)), before, "(f) nothing moves");
    for d in [music, inc] {
        std::fs::remove_dir_all(d).ok();
    }
}

/// COU.17 (c), (d) (T-COU-16): `ss-305` and `ss-306` placed "Low Tide" and "High Tide" in IN, their files' album-level tags alike
/// ((c) ALBUM the site; (d) no ALBUM at all, with the user's own "Glass Harbour Demos" `al-307` in MUSIC, no ALBUM, no DATE). Both added
/// to MUSIC (COU.15's names), their bytes as they arrived; the pass: two copies, two records, each gathered by its record and the
/// folder the gate put it in; no track moves; (d) `al-307` as it was. Red first: named from raw tags, and in MUSIC (no folder in front
/// of the hash) every file alike gathered into one album.
#[tokio::test]
async fn placed_copies_with_one_stamp_stay_their_own_records() {
    for (case, album) in [("c", Some("www.flacsite.net")), ("d", None)] {
        let (music, inc) = (folder(&format!("cou17{case}-music")), folder(&format!("cou17{case}-inc")));
        let svc = on_disk(&music, &inc).await;
        svc.lib.set_saved_template(Some(template()));
        let cat = svc.lib.catalogue();
        let demos = "Glass Harbour/Demos";
        if album.is_none() {
            let own: Vec<(String, Vec<(&str, &str)>, i64)> =
                ["Sketch", "Draft"].iter().zip([111_000, 122_000]).map(|(t, ms)| (format!("{demos}/{t}.flac"), vec![("artist", "Glass Harbour"), ("title", *t)], ms)).collect();
            files(&cat, &music, 0, &own);
            pass(&cat);
        }
        let demos_record = record_at(&cat, 0, &format!("{demos}/Sketch.flac"));
        let low = record_of_list(&cat, "Low Tide", "Glass Harbour", "R305", &L, "2016");
        let high = record_of_list(&cat, "High Tide", "Glass Harbour", "R306", &H, "2017");
        placed_arrival(&cat, &inc, low, LOW_TIDE, &L, album);
        placed_arrival(&cat, &inc, high, HIGH_TIDE, &H, album);
        pass(&cat);
        let tracks_before: Vec<Option<i64>> = ["c", "a", "e", "b", "d"].iter().map(|n| cat.track_of(1, &format!("{LOW_TIDE}/{n}.flac"))).collect();
        for (r, title, dir) in [(low, "Low Tide", LOW_TIDE), (high, "High Tide", HIGH_TIDE)] {
            svc.move_located("cratarr", located(r, title, dir, &inc), 0, crate::settings::Quality::None, None).await.unwrap_or_else(|e| panic!("({case}) {title} added: {e}"));
        }
        cat.walk_all();
        pass(&cat);
        let (low_dir, high_dir) = ("Glass Harbour/Low Tide (2016)", "Glass Harbour/High Tide (2017)");
        assert!((1..=5).all(|n| record_at(&cat, 0, &format!("{low_dir}/0{n} L{n}.flac")) == Some(low)), "({case}) Low Tide its own record");
        assert!((1..=6).all(|n| record_at(&cat, 0, &format!("{high_dir}/0{n} H{n}.flac")) == Some(high)), "({case}) High Tide its own record");
        let tracks_after: Vec<Option<i64>> = (1..=5).map(|n| cat.track_of(0, &format!("{low_dir}/0{n} L{n}.flac"))).collect();
        let mut before = tracks_before.clone();
        before.sort();
        let mut after = tracks_after.clone();
        after.sort();
        assert_eq!(after, before, "({case}) no track moves");
        if album.is_none() {
            assert!(demos_record.is_some());
            assert_eq!(record_at(&cat, 0, &format!("{demos}/Sketch.flac")), demos_record, "(d) Glass Harbour Demos as it was");
            assert_ne!(demos_record, Some(low));
            assert_ne!(demos_record, Some(high));
        }
        for d in [music, inc] {
            std::fs::remove_dir_all(d).ok();
        }
    }
}

/// Mumia's list, T1-T4: 3:10, 4:20, 5:30, 6:40.
const MUMIA: [(&str, i64); 4] = [("T1", 190_000), ("T2", 260_000), ("T3", 330_000), ("T4", 400_000)];

/// COU.7 (T-COU-5, T-COU-16; COU.15 (c)): Mumia `al-3211`, MUSIC's copy (T1-T4, numbered) and WM.10's electronicfresh arrival in IN, one
/// record, each arrival file holding the track it pairs with. (a) Replace: every old track paired with an arrival file by the track it
/// holds, each new file taking over its track's id (so every play, heart, playlist line and queue entry on T1-T4 stays); the old files
/// into MUSIC's trash; the new files' bytes as they arrived; the new folder named from the database; the new copy told as its record's;
/// Undo puts it all back. (b) "Replace and carry over": refused whole, "Quiet until 0.19", nothing moved. Red first: named from the
/// arrival's raw tags (no numbers: refused); carrying over written into the new files.
#[tokio::test]
async fn replace_by_an_untagged_arrival_keeps_every_id_and_writes_nothing() {
    let (music, inc) = (folder("cou7-music"), folder("cou7-inc"));
    let svc = on_disk(&music, &inc).await;
    svc.lib.set_saved_template(Some(template()));
    let cat = svc.lib.catalogue();
    let mumia: Vec<(String, Vec<(&str, &str)>, i64)> = MUMIA
        .iter()
        .zip(["1", "2", "3", "4"])
        .map(|((t, ms), n)| (format!("Polo & Pan/Mumia/0{n} {t}.mp3"), vec![("album", "Mumia"), ("albumartist", "Polo & Pan"), ("title", *t), ("tracknumber", n)], *ms))
        .collect();
    files(&cat, &music, 0, &mumia);
    pass(&cat);
    let r = record_at(&cat, 0, &mumia[0].0).expect("Mumia's record");
    let ids: Vec<i64> = mumia.iter().map(|(rel, ..)| cat.track_of(0, rel).unwrap()).collect();
    cat.keep_list(r, &Reference::new("musicbrainz", "release", "R3211"), &MUMIA.iter().zip(1..).map(|((t, ms), p)| RefTrack { disc: 1, position: p, title: t.to_string(), length_ms: Some(*ms), recording: None }).collect::<Vec<_>>(), 1).unwrap();
    cat.said(r, "year", "musicbrainz", "release:R3211", Some("2023"), 1).unwrap();
    cat.place_copy(1, MUMIA_DIR, r).unwrap();
    let arrival: Vec<(String, Vec<(&str, &str)>, i64)> = [("b.mp3", Some("T1")), ("c.mp3", Some("T2 www.")), ("a.mp3", Some("T3")), ("T4 - www.electronicfresh.com.mp3", None)]
        .iter()
        .zip(MUMIA)
        .map(|((name, title), (_, ms))| {
            let mut tags = vec![("album", STAMP), ("artist", "Red Axes, Polo & Pan")];
            tags.extend(title.map(|t| ("title", t)));
            (format!("{MUMIA_DIR}/{name}"), tags, ms + 600)
        })
        .collect();
    files(&cat, &inc, 1, &arrival);
    pass(&cat);
    for ((rel, ..), id) in arrival.iter().zip(&ids) {
        assert_eq!(cat.track_of(1, rel), Some(*id), "{rel} holds its track (WM.10)");
    }
    // The served library Replace finds MUSIC's copy by.
    let serve = crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
    serve.library.set_kinds(vec![(0, crate::serve::copies::Kind::Main), (1, crate::serve::copies::Kind::Incoming)]);
    serve.attach_catalogue(cat.clone());
    serve.refresh().unwrap();
    super::served_in_tests(Some(serve.clone()));
    let before = (lying(&music), lying(&inc));
    // (b) carrying over: refused whole, before anything.
    let e = svc.replace("cratarr", &crate::serve::ids::record(r), Some("in"), Some("music"), None, false, true).await.unwrap_err();
    assert_eq!(e.to_string(), super::QUIET, "(b) refused whole");
    assert_eq!((lying(&music), lying(&inc)), before, "(b) nothing moved");
    // (a) Replace.
    let at = located(r, "Mumia", MUMIA_DIR, &inc);
    let done = svc.replace_now(&at, 0, None, Vec::new(), None).await.expect("(a) every track pairs by its track; named from the record");
    let new = "Polo & Pan/Mumia (2023)";
    assert_eq!(done.folder, new, "(a) the record's names and year");
    let landed = lying(&music.join(new));
    assert_eq!(landed.keys().collect::<Vec<_>>(), ["01 T1.mp3", "02 T2.mp3", "03 T3.mp3", "04 T4.mp3"], "(a) each file its track's place and title");
    for ((rel, ..), n) in arrival.iter().zip(1usize..) {
        let to = format!("0{n} T{n}.mp3");
        assert_eq!(landed[&to].0, before.1[rel].0, "(a) {to}'s bytes as {rel} arrived: nothing written into the new copy");
        assert_eq!(cat.track_of(0, &format!("{new}/{to}")), Some(ids[n - 1]), "(a) {to} took over T{n}'s id");
    }
    let retired: i64 = cat.db_for_tests().query_row(&format!("SELECT count(*) FROM tracks WHERE retired_at IS NOT NULL AND id IN ({})", ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")), [], |r| r.get(0)).unwrap();
    assert_eq!(retired, 0, "(a) no play, heart, line or queue entry left on a retired id");
    assert!(!music.join("Polo & Pan/Mumia").join("01 T1.mp3").exists(), "(a) the old files into MUSIC's trash");
    assert_eq!(placed_in(&cat, 0, new), Some(r), "(a) the new copy told as its record's");
    // Undo puts it all back.
    let undo = done.undo.expect("its Undo");
    svc.undo(&undo).await.expect("(a) undone");
    assert_eq!((lying(&music).into_iter().filter(|(k, _)| !k.starts_with(crate::trash::TRASH_DIR)).collect::<std::collections::BTreeMap<_, _>>(), lying(&inc)), (before.0.clone(), before.1.clone()), "(a) Undo puts it all back");
    super::served_in_tests(None);
    for d in [music, inc] {
        std::fs::remove_dir_all(d).ok();
    }
}

/// 0092's one lock: a Keep never writes beside a file run. While a run holds the lock over the library's files, a Keep waits, writing
/// nothing; once the run lets go, the Keep goes on. Red first: Keep took no lock and moved the files beside the run.
#[tokio::test]
async fn a_keep_never_writes_beside_a_file_run() {
    let (music, inc) = (folder("lock-keep-music"), folder("lock-keep-inc"));
    let svc = Arc::new(on_disk(&music, &inc).await);
    svc.lib.set_saved_template(Some(template()));
    let cat = svc.lib.catalogue();
    let r = record_of_list(&cat, "Low Tide", "Glass Harbour", "R305", &L, "2016");
    placed_arrival(&cat, &inc, r, LOW_TIDE, &L, Some("www.flacsite.net"));
    pass(&cat);
    let run = crate::app::file_runs::turn(&svc.journal).await;
    let (s, at) = (svc.clone(), located(r, "Low Tide", LOW_TIDE, &inc));
    let keep = tokio::spawn(async move { s.move_located("cratarr", at, 0, crate::settings::Quality::None, None).await.map(|k| k.folder) });
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    assert!(!keep.is_finished(), "the Keep waits while a run writes");
    assert!(!music.join("Glass Harbour").exists(), "nothing written beside the run");
    drop(run);
    let done = tokio::time::timeout(std::time::Duration::from_secs(30), keep).await.expect("it goes on once the run lets go").unwrap();
    assert_eq!(done.expect("added").as_str(), "Glass Harbour/Low Tide (2016)");
    for d in [music, inc] {
        std::fs::remove_dir_all(d).ok();
    }
}
