//! What judges a copy whole, in the one place that does (`held_in`, SCENARIOS-VR01 VR8, VR13 (c), VR14, VR21, VR46, VR47, VR48;
//! VR12 rule 6): the pick alone, a guess never (but one today's pairing confirms), no tie broken by a source's name, the timed
//! lists alone while one is kept; and the facts' and lists' rows a merge and retention move (VR40, VR41).

use cratarr_domain::facts::Reference;
use cratarr_rules::holding::{Holding, Kept, Library, On, holding_of};
use rusqlite::params;

use super::Catalogue;
use super::holding::Copies;
use super::identity::RefTrack;
use crate::serve::ids;

const MUSIC: i64 = 0;
const IN: i64 = 1;

fn library(folder: i64) -> Library {
    match folder {
        IN => Library::Incoming { fetcher: cratarr_rules::holding::Fetcher::Lidarr, uploader: None },
        _ => Library::Main,
    }
}

fn catalogue() -> Catalogue {
    Catalogue::open(None, Vec::new()).unwrap()
}

fn record(cat: &Catalogue, title: &str) -> i64 {
    let db = cat.db();
    db.execute("INSERT INTO records (born, title, artist) VALUES (1, ?1, 'Glass Harbour')", [title]).unwrap();
    db.last_insert_rowid()
}

/// A track of a list: its place, its title, its length in seconds where the list gives one.
fn t(position: u32, title: &str, secs: Option<i64>) -> RefTrack {
    RefTrack { disc: 1, position, title: title.to_string(), length_ms: secs.map(|s| s * 1000), recording: None }
}

/// Salt Lines' S1-Sn, each 200 s + n × 7 s.
fn salt(n: u32) -> Vec<RefTrack> {
    (1..=n).map(|i| t(i, &format!("S{i}"), Some(salt_secs(i)))).collect()
}

fn salt_secs(i: u32) -> i64 {
    200 + i64::from(i) * 7
}

/// A list kept under its reference, the reference held with `how` (none: the list alone).
fn keep(cat: &Catalogue, record: i64, r: &Reference, how: Option<&str>, tracks: &[RefTrack]) {
    if let Some(how) = how {
        cat.db()
            .execute(
                "INSERT INTO \"references\" (subject, source, kind, value, at, how) VALUES (?1, ?2, ?3, ?4, 1, ?5)",
                params![ids::record(record), r.source, r.kind, r.value, how],
            )
            .unwrap();
    }
    cat.keep_list(record, r, tracks, 1).unwrap();
}

/// A copy in `folder`: each (place, title, length in seconds) a track and its file.
fn copy(cat: &Catalogue, record: i64, folder: i64, files: &[(u32, String, i64)]) {
    let db = cat.db();
    for (n, title, secs) in files {
        db.execute("INSERT INTO tracks (born, record, disc, number) VALUES (1, ?1, 1, ?2)", params![record, n]).unwrap();
        let track = db.last_insert_rowid();
        let rel = format!("r{record}/{folder}/{n:02}.flac");
        db.execute("INSERT INTO files (folder, rel, size, mtime, track, duration_ms) VALUES (?1, ?2, 1, 1, ?3, ?4)", params![folder, rel, track, secs * 1000]).unwrap();
        for (i, (name, value)) in [("title", title.clone()), ("tracknumber", n.to_string())].into_iter().enumerate() {
            db.execute("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (?1, ?2, ?3, ?4, ?4, ?5)", params![folder, rel, i as i64, name, value]).unwrap();
        }
    }
}

/// Salt Lines' tracks at these places, as their files (each its list length).
fn salt_files(places: impl IntoIterator<Item = u32>) -> Vec<(u32, String, i64)> {
    places.into_iter().map(|i| (i, format!("S{i}"), salt_secs(i))).collect()
}

fn of(cat: &Catalogue, record: i64) -> Copies {
    cat.copies_of(&[record], &library).unwrap().remove(&record).unwrap_or_default()
}

fn held(c: &Copies) -> Holding {
    holding_of(&c.copies, c.kept(), "alice", &On::all())
}

/// VR8's and VR46's store half (H2, VR01's resolution 1): the holding never judges by a guess. Salt Lines keeps
/// MusicBrainz's ten (read through `G70`'s `R70`, `how` "group") and Bandcamp's ten (by MusicBrainz's link), and a page 0.17
/// carried, found by name (`P70s`, "search", 7 tracks of another album); IN holds S1-S8. Red first: `held_in` read every kept list
/// whatever its `how`: the 7 the shortest, Partly { 0, 7 }.
#[test]
fn a_guess_is_never_judged_by() {
    let cat = catalogue();
    let r = record(&cat, "Salt Lines");
    keep(&cat, r, &Reference::new("musicbrainz", "release", "R70"), Some("group"), &salt(10));
    keep(&cat, r, &Reference::new("bandcamp", "album", "P70"), Some("link"), &salt(10));
    let other: Vec<RefTrack> = (1..=7).map(|i| t(i, &format!("Other {i}"), Some(100 + i64::from(i)))).collect();
    keep(&cat, r, &Reference::new("bandcamp", "album", "P70s"), Some("search"), &other);
    copy(&cat, r, IN, &salt_files(1..=8));
    let c = of(&cat, r);
    assert_eq!(c.shortest, Some(10), "never 7");
    assert_eq!(held(&c), Holding::Partly { has: 8, of: 10 }, "never Partly 0 of 7");
    // VR46 (Orbital): a guessed page kept on the had record judges neither its holding nor, as a holder's list, another record's.
    let orbital = record(&cat, "Orbital");
    keep(&cat, orbital, &Reference::new("musicbrainz", "release", "R91"), Some("files"), &salt(10));
    keep(&cat, orbital, &Reference::new("bandcamp", "album", "orbital-3"), Some("search"), &salt(6));
    copy(&cat, orbital, MUSIC, &salt_files(1..=10));
    let o = of(&cat, orbital);
    assert_eq!((o.shortest, held(&o)), (Some(10), Holding::Had), "MusicBrainz's judges, the page never");
    let asked = record(&cat, "Orbital 1993");
    let by_holder = cat.copies_held(&[(asked, orbital)], &library).unwrap().remove(&asked).unwrap();
    assert_eq!(by_holder.shortest, Some(10), "the holder's lists, the guess left out as here");
}

/// VR48's store half (the owner's 02:32, RV-22, C15, C16): a carried match is judged by today's pairing. (a) MUSIC's ten pair its
/// list one for one: a reference, Had; (b) a track missing and (c) a track 4:40 against 3:05: guesses, and with no other list kept
/// not known whole, no count; (f) an Incoming arrival pairing one for one confirms nothing until it is added. Red first: every
/// list judged whatever its `how` (no rule 6, no confirmation).
#[test]
fn a_carried_match_is_judged_by_todays_pairing() {
    let cat = catalogue();
    let matched = |title: &str, value: &str, folder: i64, files: Vec<(u32, String, i64)>| {
        let r = record(&cat, title);
        keep(&cat, r, &Reference::new("musicbrainz", "release", value), Some("search"), &salt(10));
        copy(&cat, r, folder, &files);
        r
    };
    let quay_lights = matched("Quay Lights", "R80", MUSIC, salt_files(1..=10));
    let quay_shadows = matched("Quay Shadows", "R81", MUSIC, salt_files((1..=10).filter(|i| *i != 6)));
    let mut tide = salt_files(1..=10);
    tide[3].2 = 280;
    let quay_tide = matched("Quay Tide", "R82", MUSIC, tide);
    let arrival = matched("Quay Arrival", "R84", IN, salt_files(1..=10));
    let (a, b, c, f) = (of(&cat, quay_lights), of(&cat, quay_shadows), of(&cat, quay_tide), of(&cat, arrival));
    assert_eq!((a.kept(), held(&a)), (Kept::Shortest(10), Holding::Had), "(a) confirmed whole: a reference, Had");
    for (copies, what) in [(&b, "(b) a track missing"), (&c, "(c) a track mismatched")] {
        assert_eq!((copies.kept(), held(copies)), (Kept::OnlyGuesses, Holding::Unknown), "{what}: not known whole, no count");
        assert_eq!(copies.shortest, None, "{what}: no 'of'");
    }
    assert_eq!((f.kept(), held(&f)), (Kept::OnlyGuesses, Holding::Unknown), "(f) an arrival in Incoming confirms nothing, never Arrived");
    // (f) once added to MUSIC, still one for one: a reference, the copy whole by it.
    cat.db().execute("UPDATE files SET folder = ?1 WHERE track IN (SELECT id FROM tracks WHERE record = ?2)", params![MUSIC, arrival]).unwrap();
    cat.db().execute("UPDATE tags SET folder = ?1 WHERE rel LIKE ?2", params![MUSIC, format!("r{arrival}/%")]).unwrap();
    assert_eq!(held(&of(&cat, arrival)), Holding::Had, "added: confirmed by the pairing");
    // (e) a file of (a) replaced by one whose sound runs 2:00: a guess again, worked out from the files as they are now.
    cat.db().execute("UPDATE files SET duration_ms = 120000 WHERE rel = ?1", [format!("r{quay_lights}/0/03.flac")]).unwrap();
    assert_eq!(held(&of(&cat, quay_lights)), Holding::Unknown, "(e) no stored verdict keeps it a reference");
    // A Confirm (a hand) makes it a reference whatever the pairing says.
    cat.db().execute("UPDATE \"references\" SET confirmed = 1 WHERE subject = ?1", [ids::record(quay_lights)]).unwrap();
    assert_eq!(held(&of(&cat, quay_lights)), Holding::Partly { has: 9, of: 10 });
}

/// VR13 (c) (M3): no tie broken by a source's name. Tide Book keeps MusicBrainz's ten (T10 7:40) and Discogs' ten ("T10 (Edit)"
/// 3:10); a copy of T1-T8 and T10 (7:40), then one of T1-T8 and "T10 (Edit)" (3:10): each Partly { 9, 10 }, the most pairs with
/// either. Red first: (c1) read Partly { 8, 10 } (Discogs', its name first).
#[test]
fn no_tie_is_broken_by_a_sources_name() {
    let cat = catalogue();
    let tide = |last: &str, secs: i64| (1..=10).map(|i| if i == 10 { t(10, last, Some(secs)) } else { t(i, &format!("T{i}"), Some(200 + i64::from(i))) }).collect::<Vec<_>>();
    let book = |title: &str, file_ten: (&str, i64)| {
        let r = record(&cat, title);
        keep(&cat, r, &Reference::new("musicbrainz", "release", &format!("R71 {title}")), Some("files"), &tide("T10", 460));
        keep(&cat, r, &Reference::new("discogs", "release", &format!("D71 {title}")), Some("link"), &tide("T10 (Edit)", 190));
        let mut files: Vec<(u32, String, i64)> = (1..=8).map(|i| (i, format!("T{i}"), 200 + i64::from(i))).collect();
        files.push((10, file_ten.0.to_string(), file_ten.1));
        copy(&cat, r, IN, &files);
        r
    };
    let c1 = book("Tide Book 1", ("T10", 460));
    let c2 = book("Tide Book 2", ("T10 (Edit)", 190));
    assert_eq!(held(&of(&cat, c1)), Holding::Partly { has: 9, of: 10 }, "(c1) MusicBrainz's pairs 9");
    assert_eq!(held(&of(&cat, c2)), Holding::Partly { has: 9, of: 10 }, "(c2) Discogs' pairs 9");
}

/// VR14 (T1) and VR47's store half: with a pick, the pick judges. Salt Lines keeps MusicBrainz's ten and Discogs' twelve (through
/// `M70`); root picks Discogs' twelve: IN's S1-S10 reads Partly { 10, 12 }, S1-S12 Had; after Reset the ten whole again; a pick of
/// a shorter vinyl of 8 makes a copy of those 8 whole. Red first: `held_in` read no pick.
#[test]
fn a_pick_judges_whole() {
    let cat = catalogue();
    let m70 = Reference::new("discogs", "master", "M70");
    let r = record(&cat, "Salt Lines");
    keep(&cat, r, &Reference::new("musicbrainz", "release", "R70"), Some("group"), &salt(10));
    keep(&cat, r, &m70, Some("link"), &salt(12));
    copy(&cat, r, IN, &salt_files(1..=10));
    assert_eq!(held(&of(&cat, r)), Holding::Arrived, "none picked: whole by the ten");
    cat.pick_list(r, Some(&m70)).unwrap();
    let picked = of(&cat, r);
    assert_eq!((picked.shortest, picked.picked, held(&picked)), (Some(12), true, Holding::Partly { has: 10, of: 12 }), "(a) the pick alone");
    let whole = record(&cat, "Salt Lines 12");
    keep(&cat, whole, &m70, Some("link"), &salt(12));
    // A master is every edition's (G1): `M70` named by both records.
    keep(&cat, whole, &Reference::new("musicbrainz", "release", "R70b"), Some("files"), &salt(10));
    cat.pick_list(whole, Some(&m70)).unwrap();
    copy(&cat, whole, IN, &salt_files(1..=12));
    assert_eq!(held(&of(&cat, whole)), Holding::Arrived, "(b) whole by the pick");
    cat.pick_list(r, None).unwrap();
    assert_eq!((of(&cat, r).picked, held(&of(&cat, r))), (false, Holding::Arrived), "after Reset, the shortest is ten again");
    let vinyl = Reference::new("discogs", "release", "D70v8");
    keep(&cat, r, &vinyl, Some("hand"), &salt(8));
    cat.pick_list(r, Some(&vinyl)).unwrap();
    let eight = record(&cat, "Salt Lines vinyl");
    let its_vinyl = Reference::new("discogs", "release", "D80v8");
    keep(&cat, eight, &its_vinyl, Some("hand"), &salt(8));
    keep(&cat, eight, &Reference::new("musicbrainz", "release", "R70c"), Some("files"), &salt(10));
    cat.pick_list(eight, Some(&its_vinyl)).unwrap();
    copy(&cat, eight, MUSIC, &salt_files(1..=8));
    assert_eq!(held(&of(&cat, eight)), Holding::Had, "a pick shorter than the others: whole by the hand (D.1)");
}

/// VR21 (the owner's OQ-VR-7, C12, RV-21): a list without lengths never makes a copy whole while any kept list gives lengths, and
/// sets no `of`; it judges by titles alone only where none does. (a) Discogs' ten untimed beside MusicBrainz's twelve timed, IN's
/// S10 cut to 2:00: Partly { 9, 12 }; (c) Discogs' ten untimed beside two timed tens, S10 2:00: Partly { 9, 10 }; (d) Discogs' ten
/// alone: Arrived by titles; (e) Discogs' ten untimed beside MusicBrainz's twelve, S1-S10 uncut: Partly { 10, 12 }, never whole.
/// Red first: (a), (c) and (e) read whole by Discogs' ten.
#[test]
fn a_list_without_lengths_never_makes_a_copy_whole_beside_a_timed_one() {
    let cat = catalogue();
    let untimed: Vec<RefTrack> = (1..=10).map(|i| t(i, &format!("S{i}"), None)).collect();
    let with = |title: &str, timed: &[(&str, Vec<RefTrack>)], files: Vec<(u32, String, i64)>| {
        let r = record(&cat, title);
        keep(&cat, r, &Reference::new("discogs", "release", &format!("D70 {title}")), Some("link"), &untimed);
        for (value, list) in timed {
            keep(&cat, r, &Reference::new("musicbrainz", "release", &format!("{value} {title}")), Some("files"), list);
        }
        copy(&cat, r, IN, &files);
        held(&of(&cat, r))
    };
    let mut cut = salt_files(1..=10);
    cut[9].2 = 120;
    assert_eq!(with("(a)", &[("R70", salt(12))], cut.clone()), Holding::Partly { has: 9, of: 12 }, "(a)");
    let mut bandcamp = salt(10);
    bandcamp[9].length_ms = Some(190_000);
    assert_eq!(with("(c)", &[("R70", salt(10)), ("R70b", bandcamp)], cut), Holding::Partly { has: 9, of: 10 }, "(c)");
    let mut short_ten = salt_files(1..=10);
    short_ten[9].2 = 120;
    assert_eq!(with("(d)", &[], short_ten), Holding::Arrived, "(d) no kept list gives lengths: by titles alone");
    assert_eq!(with("(e)", &[("R70", salt(12))], salt_files(1..=10)), Holding::Partly { has: 10, of: 12 }, "(e) never whole by the untimed ten");
}

/// VR3's and VR40's store half (H3): a source's "asked, none" is a `facts` row whose value is none, with its time, never an empty
/// list, and the holding reads it as no list. Red first: `facts.value` NOT NULL.
#[test]
fn asked_none_is_kept_with_its_time() {
    let cat = catalogue();
    let answered = record(&cat, "Answered");
    cat.said(answered, "tracklist", "discogs", "", None, 1_772_539_200).unwrap();
    let said = cat.facts_said(answered).unwrap();
    assert_eq!((said[0].value.as_deref(), said[0].at), (None, 1_772_539_200), "asked, none, with its time");
    keep(&cat, answered, &Reference::new("musicbrainz", "release", "R7"), Some("files"), &salt(9));
    assert!(cat.lists_kept(answered).unwrap().iter().all(|l| l.reference.source != "discogs"), "never a list of 0 for Discogs");
    assert_eq!(of(&cat, answered).shortest, Some(9));
    let empty: i64 = cat.db().query_row("SELECT count(*) FROM facts WHERE value = ''", [], |r| r.get(0)).unwrap();
    assert_eq!(empty, 0, "never an empty string standing for none");
}

/// VR40's FA13 store half (H4): a release's year is kept per reference. Thin Walls holds `R1` 2014 and `R2` 2016, both read back;
/// a re-read of `R1` leaves `R2`'s. Red first: `facts` one row per (record, fact, source).
#[test]
fn a_releases_year_is_kept_per_reference() {
    let cat = catalogue();
    let thin_walls = record(&cat, "Thin Walls");
    cat.said(thin_walls, "year", "musicbrainz", "release:R1", Some("2014"), 1).unwrap();
    cat.said(thin_walls, "year", "musicbrainz", "release:R2", Some("2016"), 1).unwrap();
    cat.said(thin_walls, "year", "musicbrainz", "release:R1", Some("2015"), 2).unwrap();
    let years: Vec<(String, Option<String>)> = cat.facts_said(thin_walls).unwrap().into_iter().map(|s| (s.of, s.value)).collect();
    assert_eq!(years, [("release:R1".to_string(), Some("2015".to_string())), ("release:R2".to_string(), Some("2016".to_string()))]);
}

/// VR41 and FA16 (kind 3): a merge moves facts and lists. `al-75` into `al-70`: Discogs' year on `M70` one row, the later; the years
/// of `R75` and `R70` both kept; Discogs' lists of one reference (`M70`) one row, the later; MusicBrainz's two releases' lists both
/// kept; `al-70`'s count stays none (al-75's worked out, never carried); nothing left on `al-75`. Red first: INSERT OR IGNORE kept
/// the target's row whatever its `at`, and the 10 was lost.
#[test]
fn a_merge_moves_its_facts() {
    let cat = catalogue();
    let (into, from) = (record(&cat, "Salt Lines"), record(&cat, "Salt Lines (deluxe)"));
    for (r, discogs_at, discogs_year) in [(into, 1, "2014"), (from, 2, "2015")] {
        cat.said(r, "year", "discogs", "master:M70", Some(discogs_year), discogs_at).unwrap();
        cat.keep_list(r, &Reference::new("discogs", "master", "M70"), &salt(12), discogs_at).unwrap();
    }
    cat.said(from, "year", "musicbrainz", "release:R75", Some("2016"), 3).unwrap();
    cat.said(into, "year", "musicbrainz", "release:R70", Some("2014"), 1).unwrap();
    cat.keep_list(from, &Reference::new("musicbrainz", "release", "R75"), &salt(10), 3).unwrap();
    cat.keep_list(into, &Reference::new("musicbrainz", "release", "R70"), &salt(9), 1).unwrap();
    cat.db().execute("UPDATE records SET counting = 'musicbrainz', counted_by_hand = 0 WHERE id = ?1", [from]).unwrap();
    cat.merge_records(from, into).unwrap();
    let facts: Vec<(String, String, Option<String>)> = cat.facts_said(into).unwrap().into_iter().map(|s| (s.source, s.of, s.value)).collect();
    assert_eq!(
        facts,
        [
            ("discogs".to_string(), "master:M70".to_string(), Some("2015".to_string())),
            ("musicbrainz".to_string(), "release:R70".to_string(), Some("2014".to_string())),
            ("musicbrainz".to_string(), "release:R75".to_string(), Some("2016".to_string())),
        ],
        "the later at stays; two releases' years both kept"
    );
    let lists: Vec<(String, usize, i64)> = cat.lists_kept(into).unwrap().into_iter().map(|l| (l.reference.value, l.tracks.len(), l.at)).collect();
    assert_eq!(lists, [("M70".to_string(), 12, 2), ("R70".to_string(), 9, 1), ("R75".to_string(), 10, 3)], "one row per reference, the later");
    assert_eq!(cat.record(into).unwrap().unwrap().counting, None, "a worked-out count is never carried");
    let left: i64 = cat.db().query_row("SELECT (SELECT count(*) FROM facts WHERE record = ?1) + (SELECT count(*) FROM reference_tracks WHERE record = ?1)", [from], |r| r.get(0)).unwrap();
    assert_eq!(left, 0, "no row left on the merged id");
}

/// VR10 (M7, RV-17): `records.counting` is written by a hand alone. Over every crate's sources (their code outside tests): no
/// `UPDATE records SET counting` but the hand's pick (`lists::pick_list`), `set_counting`'s by-hand write, and 0.17's step's carry
/// of 0.17's choices by hand; and no `set_counting(.., false)`. Red first: `keep` (records/app/references.rs) and `work_out_in`
/// worked a count out, as `recount_in` and `let_go_list` did here.
#[test]
fn counting_is_written_by_a_hand_alone() {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    // (file, why): the writes allowed, each by its reason.
    let allowed = [
        ("lists.rs", "the administrator's pick, by its reference (VR5)"),
        ("records.rs", "set_counting by hand; 0.17's step carries 0.17's choices by hand (C44)"),
        ("step_018.rs", "0.18's step: 0.17's counts worked out set to none, a hand's pick named by its reference (M7, VR43)"),
    ];
    let mut found: Vec<String> = Vec::new();
    let mut stack = vec![crates];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if path.is_dir() {
                if name != "target" {
                    stack.push(path);
                }
                continue;
            }
            if !name.ends_with(".rs") || name.ends_with("_tests.rs") || name.contains("tests_") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap().replace('\r', "");
            let code = text.split("#[cfg(test)]").next().unwrap_or("");
            for (n, line) in code.lines().enumerate() {
                let worked = line.contains("set_counting(") && line.contains(", false)");
                let write = line.contains("UPDATE records SET counting") && !allowed.iter().any(|(f, _)| *f == name);
                if worked || write {
                    found.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
                }
            }
        }
    }
    assert!(found.is_empty(), "a count written by no hand: {found:#?}");
}
