//! SCENARIOS-COUPLING COU.15's catalogue half: adding an arrival, and Replace, name the files from the record's facts in the
//! database (`Catalogue::names_from_record`), the plan the store's (`incoming::plan_keep`, `plan_replace`). On real folders, as a
//! pass reads them.

use std::collections::HashMap;

use cratarr_rules::holding::Library;

use super::wm_tests::{Folders, IN, MUMIA, MUMIA_DIR, MUSIC, electronicfresh, keep, list, mumia_in_music, record};
use crate::incoming::{AlbumFile, KeepRefusal, ReplaceRefusal, plan_keep, plan_replace};

/// MUSIC's template in COU.15: `{albumartist}/{album} ({originalyear})/{track:00} {title}`, two digits.
fn template() -> crate::naming::Template {
    "{albumartist}/{album} ({originalyear})/{track:02} {title}".parse().unwrap()
}

const LOW_TIDE: &str = "soulseek/Glass Harbour/Low Tide";
/// Low Tide's list, L1-L5, more than four seconds apart.
const L: [(&str, i64); 5] = [("L1", 200_000), ("L2", 230_000), ("L3", 260_000), ("L4", 290_000), ("L5", 320_000)];

fn library(folder: i64) -> Library {
    super::wm_tests::library(folder)
}

/// "Low Tide" `al-305`, not had: MusicBrainz's list (L1-L5) and its year 2016 kept (and, `discogs`, Discogs' 2015 too); `ss-305`'s
/// arrival placed in IN, its files as Soulseek left them: ALBUM the site, ARTIST "Glass Harbour", no ALBUMARTIST, DATE or
/// TRACKNUMBER, named in another order than the list's, each its track's title and length; and, `x9`, a sixth file with no tags
/// whose length pairs with no track. Read and passed. The record.
fn low_tide(f: &Folders, discogs: bool, x9: bool) -> i64 {
    let r = record(&f.cat, "Low Tide", "Glass Harbour");
    keep(&f.cat, r, "R305", &list(&L));
    f.cat.said(r, "year", "musicbrainz", "release:R305", Some("2016"), 1).unwrap();
    if discogs {
        f.cat.said(r, "year", "discogs", "release:D305", Some("2015"), 1).unwrap();
    }
    f.cat.place_copy(IN, LOW_TIDE, r).unwrap();
    let names = ["c", "a", "e", "b", "d"];
    let mut lengths = Vec::new();
    for ((title, ms), name) in L.iter().zip(names) {
        let rel = format!("{LOW_TIDE}/{name}.flac");
        f.add(IN, &[(rel.as_str(), vec![("album", "www.flacsite.net"), ("artist", "Glass Harbour"), ("title", title)])]);
        lengths.push((rel, *ms));
    }
    f.lengths(IN, &lengths.iter().map(|(rel, ms)| (rel.as_str(), *ms)).collect::<Vec<_>>());
    f.pass();
    assert_eq!(f.record_of(IN, &format!("{LOW_TIDE}/c.flac")), Some(r), "the arrival joined its item's record");
    if x9 {
        // A sixth file come into the arrival's folder since.
        let rel = format!("{LOW_TIDE}/x9.flac");
        f.add(IN, &[(rel.as_str(), Vec::new())]);
        f.lengths(IN, &[(rel.as_str(), 610_000)]);
        f.pass();
        assert_eq!(f.record_of(IN, &rel), Some(r), "a file of its copy");
    }
    r
}

/// The names the database gives the copy at `dir` in `folder`, MUSIC deciding, `hand` the record's title set by hand.
fn named(f: &Folders, folder: i64, dir: &str, hand: Option<&str>) -> Vec<(String, crate::naming::Tags)> {
    let hand = hand.map(str::to_string);
    f.cat.names_from_record(folder, dir, &[MUSIC], &|_| hand.clone(), &|_| HashMap::new(), &library).unwrap()
}

fn moves(plan: &crate::incoming::KeepPlan) -> Vec<String> {
    let mut to: Vec<String> = plan.moves.iter().map(|(_, to)| to.clone()).collect();
    to.sort();
    to
}

/// COU.15 (a), (b), (d), (e): "Low Tide" `al-305`, its arrival joined by its item. (a) Adding it to MUSIC names every file from the
/// record: `Glass Harbour/Low Tide (2016)/01 L1.flac` … `05 L5.flac`, its year MusicBrainz's, each number its pairing's, each title its
/// track's; the ALBUM "www.flacsite.net" names nothing; (b) with Discogs' 2015 beside MusicBrainz's 2016, none chosen, the year is
/// asked, and given, as (a); (d) a sixth file the database can't number is refused, the refusal naming it; (e) the hand's title
/// names the folder. (c) COU.7's Replace into MUSIC: Mumia's new folder named from `al-3211`'s facts, each file its track's place and
/// title (`T2 www.` and the untitled fourth among them). Red first: the files' raw tags: the folder "www.flacsite.net", no year and
/// no numbers to name by.
#[test]
fn adding_names_the_files_from_the_record() {
    // (a)
    let f = Folders::new("cou15a");
    low_tide(&f, false, false);
    let plan = plan_keep(&template(), &named(&f, IN, LOW_TIDE, None), &[], None).expect("(a) named, nothing asked");
    assert_eq!(plan.folder, "Glass Harbour/Low Tide (2016)", "(a) the record's credit, title and year; the stamp names nothing");
    let want: Vec<String> = (1..=5).map(|n| format!("Glass Harbour/Low Tide (2016)/0{n} L{n}.flac")).collect();
    assert_eq!(moves(&plan), want, "(a) each file its pairing's number and its track's title");
    drop(f);
    // (b)
    let f = Folders::new("cou15b");
    low_tide(&f, true, false);
    let names = named(&f, IN, LOW_TIDE, None);
    assert_eq!(plan_keep(&template(), &names, &[], None), Err(KeepRefusal::NeedsYear), "(b) the sources differ, none chosen: asked");
    assert_eq!(plan_keep(&template(), &names, &[], Some("2016")).unwrap().folder, "Glass Harbour/Low Tide (2016)", "(b) given, as (a)");
    drop(f);
    // (d)
    let f = Folders::new("cou15d");
    low_tide(&f, false, true);
    match plan_keep(&template(), &named(&f, IN, LOW_TIDE, None), &[], None) {
        Err(KeepRefusal::Names(why)) => assert!(why.contains("x9.flac") && why.contains("track number"), "(d) the refusal names the file and its number: {why}"),
        other => panic!("(d) refused for x9.flac's number: {other:?}"),
    }
    drop(f);
    // (e)
    let f = Folders::new("cou15e");
    low_tide(&f, false, false);
    let plan = plan_keep(&template(), &named(&f, IN, LOW_TIDE, Some("Low Tide (Live)")), &[], None).unwrap();
    assert_eq!(plan.folder, "Glass Harbour/Low Tide (Live) (2016)", "(e) the hand's title");
    drop(f);
    // (c) Mumia's arrival, joined (WM.10), replaces MUSIC's copy.
    let f = Folders::new("cou15c");
    let r = mumia_in_music(&f);
    f.cat.said(r, "year", "musicbrainz", "release:R3211", Some("2023"), 1).unwrap();
    f.cat.place_copy(IN, MUMIA_DIR, r).unwrap();
    electronicfresh(&f, false);
    f.pass();
    let files = |folder: i64, dir: &str| -> Vec<AlbumFile> {
        f.cat
            .copy_read(folder, dir)
            .unwrap()
            .into_iter()
            .map(|(rel, tags, ms)| AlbumFile { track: f.cat.track_of(folder, &rel), rel: rel.strip_prefix(&format!("{dir}/")).unwrap_or(&rel).to_string(), tags: crate::naming::Tags(tags), duration_ms: ms })
            .collect()
    };
    let (old, new) = (files(MUSIC, "Polo & Pan/Mumia"), files(IN, MUMIA_DIR));
    let plan = plan_replace(&template(), &old, &new, &named(&f, IN, MUMIA_DIR, None), &[], None).expect("(c) every track pairs by its track; named from the record");
    assert_eq!(plan.folder, "Polo & Pan/Mumia (2023)", "(c) the record's names and year");
    let mut to: Vec<String> = plan.moves.iter().map(|(_, to)| to.clone()).collect();
    to.sort();
    let want: Vec<String> = MUMIA.iter().zip(1..).map(|((t, _), n)| format!("Polo & Pan/Mumia (2023)/0{n} {t}.mp3")).collect();
    assert_eq!(to, want, "(c) each file its track's place and title");
    assert_eq!(plan.takes_over.len(), 4, "(c) every old track taken over");
    assert!(!matches!(plan_replace(&template(), &old, &new, &named(&f, IN, MUMIA_DIR, None), &[], None), Err(ReplaceRefusal::Names(_))));
}

/// T-COU-14's catalogue half: the records whose files lie under a folder, so adding a copy into a folder another copy of its record
/// holds is told apart from a folder of another record's.
#[test]
fn a_folder_names_the_records_it_holds() {
    let f = Folders::new("cou15f");
    let r = low_tide(&f, false, false);
    let held = "Glass Harbour/Low Tide (2016)";
    f.add(MUSIC, &[(format!("{held}/01 L1.mp3").as_str(), vec![("album", "Low Tide"), ("albumartist", "Glass Harbour"), ("title", "L1"), ("tracknumber", "1"), ("date", "2016")])]);
    f.cat.place_copy(MUSIC, held, r).unwrap();
    f.pass();
    assert_eq!(f.cat.records_under(MUSIC, held).unwrap(), [r]);
    assert!(f.cat.records_under(MUSIC, "Glass Harbour/High Tide (2017)").unwrap().is_empty());
}
