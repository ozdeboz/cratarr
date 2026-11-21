//! 0.18's step, its part `facts` (SCENARIOS-VR01 VR42, VR43): the lists keyed by reference, the relabels, the counts and the years
//! carried once, whole or nothing.

use rusqlite::params;

use super::Catalogue;
use super::step_018::{FACTS, FactsCarry};

const T: i64 = 1_772_539_200;

fn record(cat: &Catalogue, id: i64, title: &str) {
    record_of(cat, id, title);
}

fn record_of(cat: &Catalogue, id: i64, title: &str) {
    cat.db().execute("INSERT INTO records (id, born, title, artist) VALUES (?1, 1, ?2, 'Kite Theory')", params![id, title]).unwrap();
}

fn reference(cat: &Catalogue, record: i64, source: &str, kind: &str, value: &str, how: &str) {
    cat.db()
        .execute("INSERT INTO \"references\" (subject, source, kind, value, at, how) VALUES (?1, ?2, ?3, ?4, 1, ?5)", params![format!("al-{record}"), source, kind, value, how])
        .unwrap();
}

/// A list 0.17 kept: one per source, keyed by none.
fn list_017(cat: &Catalogue, record: i64, source: &str, n: u32) {
    let tracks: Vec<serde_json::Value> = (1..=n).map(|i| serde_json::json!({ "disc": 1, "position": i, "title": format!("T{i}"), "length_ms": 200_000 })).collect();
    cat.db()
        .execute("INSERT INTO reference_tracks (record, source, tracks, at) VALUES (?1, ?2, ?3, ?4)", params![record, source, serde_json::Value::Array(tracks).to_string(), T])
        .unwrap();
}

fn how(cat: &Catalogue, record: i64, value: &str) -> String {
    cat.db().query_row("SELECT how FROM \"references\" WHERE subject = ?1 AND value = ?2", params![format!("al-{record}"), value], |r| r.get(0)).unwrap()
}

fn keys(cat: &Catalogue, record: i64) -> Vec<(String, String, String)> {
    cat.lists_kept(record).unwrap().into_iter().map(|l| (l.reference.source, l.reference.kind, l.reference.value)).collect()
}

/// Production's shapes, as VR42 lists them: X2's fixtures (x1)-(x4), a list of a source the record holds no reference of, and the
/// three kinds of count.
fn copy_017() -> (Catalogue, FactsCarry) {
    let cat = Catalogue::open(None, Vec::new()).unwrap();
    // (x1) al-90: Lidarr's by-name group G90 and R90 read through it.
    record(&cat, 90, "Ninety");
    reference(&cat, 90, "musicbrainz", "release-group", "G90", "search");
    reference(&cat, 90, "musicbrainz", "release", "R90", "search");
    // (x2) al-91: a carried 0.16 match R91, its group G91 beside it, and R91r read through G91 by 0.17.
    record(&cat, 91, "Ninety-one");
    reference(&cat, 91, "musicbrainz", "release", "R91", "search");
    reference(&cat, 91, "musicbrainz", "release-group", "G91", "search");
    reference(&cat, 91, "musicbrainz", "release", "R91r", "search");
    // (x3) al-92: a refused 0.16 match's group G92 put beside it, and R92r read through it.
    record(&cat, 92, "Ninety-two");
    reference(&cat, 92, "musicbrainz", "release-group", "G92", "search");
    reference(&cat, 92, "musicbrainz", "release", "R92r", "search");
    // (x4) al-93: its group G93 asked by its id, R93 read through it; its 0.17 list, and its count by hand.
    record(&cat, 93, "Ninety-three");
    reference(&cat, 93, "musicbrainz", "release-group", "G93", "search");
    reference(&cat, 93, "musicbrainz", "release", "R93", "search");
    list_017(&cat, 93, "musicbrainz", 9);
    cat.db().execute("UPDATE records SET counting = 'musicbrainz', counted_by_hand = 1 WHERE id = 93", []).unwrap();
    // al-94: a Discogs list kept, no Discogs reference on the record.
    record(&cat, 94, "Ninety-four");
    list_017(&cat, 94, "discogs", 11);
    // al-95: a count by hand naming AcoustID (0.16's), which keeps no list; al-96: a count 0.17 worked out.
    record(&cat, 95, "Ninety-five");
    cat.db().execute("UPDATE records SET counting = 'acoustid', counted_by_hand = 1, checked_by_hand = 1 WHERE id = 95", []).unwrap();
    record(&cat, 96, "Ninety-six");
    reference(&cat, 96, "discogs", "master", "M96", "link");
    list_017(&cat, 96, "discogs", 12);
    cat.db().execute("UPDATE records SET counting = 'discogs', counted_by_hand = 0 WHERE id = 96", []).unwrap();
    // al-97: Discogs' year on its master, MusicBrainz's on its match's release; al-98: a match kept without a release.
    record(&cat, 97, "Ninety-seven");
    reference(&cat, 97, "discogs", "master", "M97", "link");
    record(&cat, 98, "Ninety-eight");
    let carry = FactsCarry {
        discogs: vec![(97, Some(2015), T), (96, None, T)],
        matches: vec![(91, Some("R91".into()), Some(2013), T), (97, Some("R97".into()), Some(2014), T), (98, None, None, T)],
        asked: vec![(93, "G93".into())],
    };
    (cat, carry)
}

/// VR42 (H1, H5, N2, X1, X2, M7): the lists keyed by reference and the relabels. (x1)-(x3) stay guesses; (x4) "asked" and "group";
/// al-93's list keyed by `R93`, its count by hand named by that reference; al-94's list listed and not carried; al-95's AcoustID
/// count listed, no pick; al-96's worked-out count none, its list keyed by `M96`. Red first: no part `facts` (0.17's rows one per
/// source, `keep`'s "search" for every group read).
#[test]
fn the_018_step_keys_lists_by_reference_and_relabels_group_reads() {
    let (cat, carry) = copy_017();
    let report = cat.step_018_facts(&carry, &|| Ok(())).unwrap();
    assert_eq!((how(&cat, 90, "R90"), how(&cat, 90, "G90")), ("search".to_string(), "search".to_string()), "(x1) a guessed group's read release stays a guess");
    assert_eq!((how(&cat, 91, "R91"), how(&cat, 91, "R91r")), ("search".to_string(), "search".to_string()), "(x2) never relabelled for its origin (X1)");
    assert_eq!(how(&cat, 92, "R92r"), "search", "(x3)");
    assert_eq!((how(&cat, 93, "G93"), how(&cat, 93, "R93")), ("asked".to_string(), "group".to_string()), "(x4) asked, then its read release a group's");
    assert_eq!(keys(&cat, 93), [("musicbrainz".to_string(), "release".to_string(), "R93".to_string())], "keyed by its main reference");
    assert_eq!(cat.picked(93).unwrap(), Some(cratarr_domain::facts::Reference::new("musicbrainz", "release", "R93")), "the hand's pick, by its reference");
    assert!(keys(&cat, 94).is_empty(), "no reference to key it by: not carried");
    assert!(report.listed.values().flatten().any(|l| l == "al-94 discogs"), "listed: {report:?}");
    assert_eq!(cat.picked(95).unwrap(), None, "AcoustID keeps no list: no pick");
    assert!(report.listed.values().flatten().any(|l| l == "al-95 acoustid"));
    assert_eq!(cat.record(96).unwrap().unwrap().counting, None, "a worked-out count set to none");
    assert_eq!(keys(&cat, 96), [("discogs".to_string(), "master".to_string(), "M96".to_string())], "its list stays, keyed");
    assert_eq!(report.counts.get("relabelled asked"), Some(&1));
    assert_eq!(report.counts.get("relabelled group"), Some(&1));
    assert_eq!(report.counts.get("counts worked out, set to none"), Some(&1));
}

/// VR42's years (H4, RV-5) and FA15's carry: Discogs' year on its Discogs main reference; MusicBrainz's from `album_matches` on the
/// release its match names; a match without a release is MusicBrainz's "asked, none"; Discogs' "none" kept with its time, counted
/// apart where no Discogs reference keys it. The report balances: rows read, rows written. Red first: no part `facts`.
#[test]
fn the_018_facts_part_carries_every_sources_answer() {
    let (cat, carry) = copy_017();
    let report = cat.step_018_facts(&carry, &|| Ok(())).unwrap();
    let said = |r: i64| cat.facts_said(r).unwrap().into_iter().map(|s| (s.source, s.of, s.value)).collect::<Vec<_>>();
    assert_eq!(
        said(97),
        [("discogs".to_string(), "master:M97".to_string(), Some("2015".to_string())), ("musicbrainz".to_string(), "release:R97".to_string(), Some("2014".to_string()))]
    );
    assert_eq!(said(98), [("musicbrainz".to_string(), String::new(), None)], "asked, none");
    assert_eq!(said(96), [("discogs".to_string(), "master:M96".to_string(), None)], "Discogs' none, on its reference, with its time");
    assert_eq!(said(91), [("musicbrainz".to_string(), "release:R91".to_string(), Some("2013".to_string()))], "a guess's year kept on its reference");
    assert_eq!(report.counts.get("discogs_facts read"), report.counts.get("years written: discogs"), "Discogs balances");
    assert_eq!(report.counts.get("album_matches read"), report.counts.get("years written: musicbrainz"), "MusicBrainz balances, never 0");
    assert_eq!(report.counts.get("album_matches read"), Some(&3));
}

/// VR43 (kind 4): the part cut off before its mark writes nothing, and the next start runs it whole; run to its end, a second start
/// runs nothing of it, the report the first's. Red first: no part.
#[test]
fn the_facts_part_is_whole_or_nothing_and_runs_once() {
    let (cat, carry) = copy_017();
    assert!(cat.step_018_facts(&carry, &|| anyhow::bail!("cut off")).is_err());
    assert_eq!(how(&cat, 93, "G93"), "search", "(a) nothing of it written");
    let unkeyed: i64 = cat.db().query_row("SELECT count(*) FROM reference_tracks WHERE kind = ''", [], |r| r.get(0)).unwrap();
    assert_eq!(unkeyed, 3, "(a) the lists as 0.17 kept them");
    let marked: i64 = cat.db().query_row("SELECT count(*) FROM meta WHERE key = ?1", [FACTS], |r| r.get(0)).unwrap();
    assert_eq!(marked, 0);
    let first = cat.step_018_facts(&carry, &|| Ok(())).unwrap();
    assert_eq!(how(&cat, 93, "G93"), "asked", "the next start runs it whole");
    let second = cat.step_018_facts(&carry, &|| panic!("(b) nothing of it runs again")).unwrap();
    assert_eq!(second, first, "(b) the first's counts");
    assert_eq!(cat.picked(93).unwrap().map(|p| p.value), Some("R93".to_string()), "a pick by hand never lost");
}

/// FA22 (F40; RG2-wide): 0.18's part `fold` joins one tracklist's two records once, its mark listing the merge; the second run does
/// nothing; the merged id answers as the record it went into (RV3: no al- id Home Assistant may hold answers 404). Red first: no
/// part (0.17's fold joined an Incoming-alone record to a main one only: two records in MUSIC stayed two).
#[test]
fn the_018_fold_joins_one_tracklists_records_once() {
    use crate::settings::LibraryKind;
    let cat = Catalogue::open(None, Vec::new()).unwrap();
    let artist = {
        let db = cat.db();
        db.execute("INSERT INTO artists (born, name, key) VALUES (1, 'Kite Theory', 'kitetheory')", []).unwrap();
        db.last_insert_rowid()
    };
    for (record, dir) in [(1790, "A/Thin Walls"), (2899, "B/Thin Walls (FLAC)")] {
        record_of(&cat, record, "Thin Walls");
        cat.db().execute("INSERT INTO record_artists (record, artist) VALUES (?1, ?2)", params![record, artist]).unwrap();
        for n in 1..=9u32 {
            let db = cat.db();
            db.execute("INSERT INTO tracks (born, record, disc, number) VALUES (1, ?1, 1, ?2)", params![record, n]).unwrap();
            let track = db.last_insert_rowid();
            let rel = format!("{dir}/{n:02}.flac");
            db.execute("INSERT INTO files (folder, rel, size, mtime, track, duration_ms) VALUES (0, ?1, 1, 1, ?2, ?3)", params![rel, track, 200_000 + i64::from(n) * 1_000]).unwrap();
            for (i, (name, value)) in [("title", format!("Wall {n}")), ("tracknumber", n.to_string())].into_iter().enumerate() {
                db.execute("INSERT INTO tags (folder, rel, n, raw, name, value) VALUES (0, ?1, ?2, ?3, ?3, ?4)", params![rel, i as i64, name, value]).unwrap();
            }
        }
    }
    let kinds = [(0, LibraryKind::Main)];
    let folded = cat.fold_018(&kinds, &|| Ok(())).unwrap().expect("run now");
    assert_eq!(folded, [(2899, 1790)], "one record, the older");
    assert_eq!(cat.record(2899).unwrap().unwrap().id, 1790, "the merged id answers as the record it went into");
    let mark: String = cat.db().query_row("SELECT value FROM meta WHERE key = ?1", [super::step_018::FOLD], |r| r.get(0)).unwrap();
    assert!(mark.contains("al-2899 into al-1790"), "the merge listed for the HA session: {mark}");
    assert_eq!(cat.fold_018(&kinds, &|| panic!("never twice")).unwrap(), None, "the second run does nothing");
}
