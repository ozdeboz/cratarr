//! HD10's characterization vectors (SCENARIOS-DC1 HD10, DOMAIN-CORE C.7): today's record had rules against `holding()` over the
//! one reader's copies, for every combination of a record's copies the scenario names, three viewers, and the switch. They agree but
//! on the rows HD10 names (HD2, HD3, HD6, HD7, HD9); the rows are written to rules' tests/vectors/had.json.
//!
//! Today's rules, as they stood before HD15 (`holding_of` with `in_view`, app/requests.rs at 6411130b7), are kept here alone, as
//! the vectors' baseline: no code reads them. `RECORD_COLUMNS`' `had` is read as the catalogue answers it (`Catalogue::record`).
//! Discover's `Have::has` and `LIBRARY_ARTISTS` wait for DC3 (the review's Low 9) and are not among them.

use cratarr_domain::holding::{Fetcher, Holding, Library, On, holding};
use serde::Serialize;

use crate::catalogue::Catalogue;
use crate::catalogue::records::Requested;
use crate::settings::LibraryKind;

const MUSIC: i64 = 0;
const IN: i64 = 1;
const OWN_A: i64 = 2;
const OWN_B: i64 = 3;
const UPL: i64 = 6;
const PARKED: i64 = -1;
/// When the gone and trashed copies went.
const T: i64 = 1_772_539_200;

/// Today's kinds: an upload's library is an Incoming one (UP01-UP03 are LB's), an own folder its account's.
fn kind(folder: i64) -> LibraryKind {
    match folder {
        MUSIC => LibraryKind::Main,
        OWN_A | OWN_B => LibraryKind::Own,
        _ => LibraryKind::Incoming,
    }
}

/// The holding's libraries: the upload is bob's (H3, UP03).
fn library(folder: i64) -> Library {
    match folder {
        MUSIC => Library::Main,
        OWN_A => Library::Own("alice".into()),
        OWN_B => Library::Own("bob".into()),
        UPL => Library::Incoming { fetcher: Fetcher::Uploads, uploader: Some("bob".into()) },
        _ => Library::Incoming { fetcher: Fetcher::Lidarr, uploader: None },
    }
}

/// Today's request rule, `holding_of` (app/requests.rs at 6411130b7), as it stood.
fn holding_of(q: &Requested) -> &'static str {
    if q.holder.is_none() || q.present == 0 {
        return "none";
    }
    let whole = |n: usize| q.list.is_none_or(|l| n >= l);
    if q.copies.is_empty() {
        return if whole(q.present) { "done" } else { "partly" };
    }
    let whole_in: Vec<LibraryKind> = q.copies.iter().filter(|(_, n)| whole(*n)).map(|(folder, _)| kind(*folder)).collect();
    if whole_in.is_empty() {
        "partly"
    } else if whole_in.iter().any(|k| *k != LibraryKind::Incoming) {
        "done"
    } else {
        "arrived"
    }
}

/// Today's `in_view` (I05), as it stood: a whole copy in a main library or the asker's own music.
fn in_view(q: &Requested, own: &[i64]) -> bool {
    let whole = |n: usize| q.list.is_none_or(|l| n >= l);
    q.copies.is_empty() || q.copies.iter().any(|(folder, n)| whole(*n) && (kind(*folder) == LibraryKind::Main || own.contains(folder)))
}

/// `records_held`'s answer before HD15: done but not in view is elsewhere. A partly holding's count as today's (`present` across
/// copies, of the counted list).
fn today(q: &Requested, viewer: &str) -> String {
    let own: Vec<i64> = match viewer {
        "alice" => vec![OWN_A],
        "bob" => vec![OWN_B],
        _ => Vec::new(),
    };
    match holding_of(q) {
        "done" if !in_view(q, &own) => "elsewhere".into(),
        "partly" => format!("partly {} of {}", q.present, q.list.unwrap_or(q.present)),
        said => said.into(),
    }
}

/// The new holding in today's words, for the comparison: retired reads as none (today's requests have no retired).
fn said(h: Holding) -> String {
    match h {
        Holding::Had => "done".into(),
        Holding::Elsewhere => "elsewhere".into(),
        Holding::Arrived => "arrived".into(),
        Holding::Partly { has, of } => format!("partly {has} of {of}"),
        Holding::NotHad | Holding::Retired { .. } | Holding::Unknown => "none".into(),
    }
}

/// One row of had.json.
#[derive(Debug, Serialize)]
struct Row {
    case: String,
    viewer: &'static str,
    /// MUSIC switched off from a place that follows the switch.
    music_off: bool,
    /// Today: the request rule's answer, the record row's `had`, and a retired record's line ("Was in Music until" its pass's day).
    today: String,
    today_had: bool,
    today_retired: Option<(i64, &'static str)>,
    /// `holding()` over `copies_of`.
    holding: Holding,
    /// The HD10 row that names a disagreement, if any.
    named: Option<&'static str>,
}

struct Case {
    name: String,
    record: i64,
    named: Option<&'static str>,
}

/// A record's track `n`'s file in `folder`, tagged with its title, its length the list's (`200 000 + n × 7 000` ms).
fn file(cat: &Catalogue, record: i64, track: i64, folder: i64, n: u32, title: &str) {
    let rel = format!("r{record}/{folder}/{n:02}.flac");
    cat.execute_for_tests(&format!(
        "INSERT INTO files (folder, rel, size, mtime, track, duration_ms) VALUES ({folder}, '{rel}', 1, 1, {track}, {});
         INSERT INTO tags (folder, rel, n, raw, name, value) VALUES ({folder}, '{rel}', 0, 'TITLE', 'title', '{title}');
         INSERT INTO tags (folder, rel, n, raw, name, value) VALUES ({folder}, '{rel}', 1, 'TRACKNUMBER', 'tracknumber', '{n}');",
        200_000 + i64::from(n) * 7_000
    ));
}

#[test]
fn the_record_had_rules_agree_with_holding_but_the_named_rows() {
    let cat = Catalogue::open(None, Vec::new()).unwrap();
    let list: Vec<crate::catalogue::identity::RefTrack> = (1..=10)
        .map(|n| crate::catalogue::identity::RefTrack { disc: 1, position: n, title: format!("Night Ferry {n}"), length_ms: Some(200_000 + i64::from(n) * 7_000), recording: None })
        .collect();
    let mut cases: Vec<Case> = Vec::new();
    // Each record: its copies as (folder, the places of the list it holds, extra files of no list's titles), how they went, a list.
    let mut make = |name: String, copies: &[(i64, std::ops::RangeInclusive<u32>, u32)], went: Option<&str>, listed: bool, named: Option<&'static str>| {
        let record = cat.record_for_tests(&name, 0);
        if listed {
            cat.set_reference_tracks(record, "musicbrainz", &list).unwrap();
        }
        let mut tracks = std::collections::BTreeMap::new();
        for (folder, places, extra) in copies {
            for n in places.clone() {
                let track = *tracks.entry(n).or_insert_with(|| cat.track_for_tests(record, n));
                file(&cat, record, track, *folder, n, &format!("Night Ferry {n}"));
            }
            for k in 0..*extra {
                let n = 11 + k;
                let track = cat.track_for_tests(record, n);
                file(&cat, record, track, *folder, n, &format!("Harbour Lights {k}"));
            }
        }
        match went {
            Some("gone") => cat.execute_for_tests(&format!("UPDATE files SET gone_at = {T} WHERE track IN (SELECT id FROM tracks WHERE record = {record})")),
            Some("trashed") => cat.execute_for_tests(&format!("UPDATE files SET trashed = '{T}/x' WHERE track IN (SELECT id FROM tracks WHERE record = {record})")),
            _ => {}
        }
        cases.push(Case { name, record, named });
    };
    for (folder, place) in [(MUSIC, "MUSIC"), (IN, "IN"), (OWN_A, "OWN-A"), (OWN_B, "OWN-B"), (UPL, "UPL"), (PARKED, "parked")] {
        for listed in [true, false] {
            let l = if listed { "a list" } else { "no list" };
            make(format!("{place} whole, {l}"), &[(folder, 1..=10, 0)], None, listed, None);
            make(format!("{place} short, {l}"), &[(folder, 1..=7, 0)], None, listed, None);
            // Ten files, eight the list's (HD2): whole by its count today.
            make(format!("{place} ten files, eight the list's, {l}"), &[(folder, 1..=8, 2)], None, listed, listed.then_some("HD2"));
            make(format!("{place} gone, {l}"), &[(folder, 1..=10, 0)], Some("gone"), listed, None);
            make(format!("{place} trashed, {l}"), &[(folder, 1..=10, 0)], Some("trashed"), listed, None);
        }
    }
    make("none, ever".into(), &[], None, true, None);
    make("MUSIC 1-6 and IN 5-10 (HD3)".into(), &[(MUSIC, 1..=6, 0), (IN, 5..=10, 0)], None, true, Some("HD3"));
    make("IN whole and OWN-B whole (HD5)".into(), &[(IN, 1..=10, 0), (OWN_B, 1..=10, 0)], None, true, None);
    make("MUSIC whole and IN whole".into(), &[(MUSIC, 1..=10, 0), (IN, 1..=10, 0)], None, true, None);
    make("IN whole and MUSIC 7".into(), &[(IN, 1..=10, 0), (MUSIC, 1..=7, 0)], None, true, None);
    let ids: Vec<i64> = cases.iter().map(|c| c.record).collect();
    let read = cat.requested(&ids).unwrap();
    let copies = cat.copies_of(&ids, &library).unwrap();
    let mut rows = Vec::new();
    let mut unnamed = Vec::new();
    for c in &cases {
        let q = &read[&c.record];
        let row = cat.record(c.record).unwrap().unwrap();
        let held = &copies[&c.record];
        let went = c.name.contains("gone") || c.name.contains("trashed");
        let upload = c.name.starts_with("UPL") && !went;
        for viewer in ["alice", "bob", "root"] {
            for music_off in [false, true] {
                let on = if music_off { On::without(&[MUSIC]) } else { On::all() };
                let new = holding(&held.copies, held.shortest, viewer, &on);
                let was = today(q, viewer);
                let today_retired = row.retired_at.map(|at| (at, "Music"));
                let (now_said, had_now) = (said(new), !matches!(new, Holding::NotHad | Holding::Retired { .. }));
                let differs_holding = now_said != was;
                // Root sees every copy: the record row's had against the new holding's being any but not had.
                let differs_had = viewer == "root" && !music_off && row.had != had_now;
                let differs_retired = matches!(new, Holding::Retired { .. }) != today_retired.is_some();
                let named = if !(differs_holding || differs_had || differs_retired) {
                    None
                } else if went && differs_retired && !differs_holding && !differs_had {
                    Some("HD9")
                } else if music_off && differs_holding && said(holding(&held.copies, held.shortest, viewer, &On::all())) == was {
                    Some("HD7")
                } else if upload && viewer != "bob" {
                    Some("HD6")
                } else {
                    c.named
                };
                if (differs_holding || differs_had || differs_retired) && named.is_none() {
                    unnamed.push(format!("{} / {viewer} / music off {music_off}: today {was} (had {}), holding {new:?}", c.name, row.had));
                }
                rows.push(Row { case: c.name.clone(), viewer, music_off, today: was, today_had: row.had, today_retired, holding: new, named });
            }
        }
    }
    assert!(unnamed.is_empty(), "today's rules and holding() disagree off the named rows:\n{}", unnamed.join("\n"));
    // Each named row is seen at least once: the vectors cover what HD10 names.
    for name in ["HD2", "HD3", "HD6", "HD7", "HD9"] {
        assert!(rows.iter().any(|r| r.named == Some(name)), "{name} named by no row");
    }
    let json = serde_json::to_string_pretty(&rows).unwrap() + "\n";
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/tests/vectors/had.json");
    if std::fs::read_to_string(&path).ok().as_deref() != Some(json.as_str()) {
        std::fs::write(&path, json).unwrap();
    }
}
