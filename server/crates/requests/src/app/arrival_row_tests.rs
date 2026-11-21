//! SCENARIOS-F53's app half: an arrival's "Which release" row as its read works it out from what is kept (F53.2–F53.5, F53.8–F53.10,
//! F53.12), and the arrival's job with the reads that make nothing and the row's Confirm (F53.7). Folder 0 is a main library (MUSIC)
//! and folder 1 an Incoming one (IN2), as the served kinds read where none are set (`kind_in`).

use std::sync::Arc;

use cratarr_domain::facts::{RefTrack, Reference};
use cratarr_domain::which_release::{State, What, track_says};

use super::intake_card::{Check, Confirmed, Row, confirm, suggested, which_release_row};
use crate::catalogue::identity::Ref;
use crate::serve::ids;

const MUSIC: i64 = 0;
const IN2: i64 = 1;

/// Track n's length: 200 s and n s.
fn ms(n: u32) -> i64 {
    200_000 + i64::from(n) * 1_000
}

/// A server of the tests' own, its catalogue in memory.
fn served() -> Arc<crate::serve::Serve> {
    let serve = crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
    serve.attach_catalogue(Arc::new(crate::catalogue::Catalogue::open(None, Vec::new()).unwrap()));
    serve
}

struct Fx {
    served: Arc<crate::serve::Serve>,
}

impl Fx {
    fn new() -> Fx {
        Fx { served: served() }
    }

    fn cat(&self) -> &Arc<crate::catalogue::Catalogue> {
        self.served.catalogue()
    }

    fn record(&self, title: &str) -> i64 {
        self.cat().record_for_tests(title, 0)
    }

    /// A copy of `record` in `folder`: each file (its place, its TITLE, its length) on a track of its own.
    fn copy(&self, record: i64, folder: i64, files: &[(u32, String, i64)]) {
        for (n, title, ms) in files {
            let track = self.cat().track_for_tests(record, *n);
            let number = n.to_string();
            self.cat().file_for_tests(folder, &format!("r{record}/{folder}/{n:02} {title}.mp3"), track, *ms, &[("title", title.as_str()), ("tracknumber", number.as_str())]);
        }
    }

    fn refer(&self, record: i64, source: &str, kind: &str, value: &str, how: &str) {
        assert!(self.cat().set_reference(&ids::record(record), &Ref::new(source, kind, value), how).unwrap(), "{source} {value} kept");
    }

    fn keep(&self, record: i64, source: &str, kind: &str, value: &str, tracks: &[(String, Option<i64>)]) {
        let list: Vec<RefTrack> = tracks.iter().zip(1..).map(|((title, ms), position)| RefTrack { disc: 1, position, title: title.clone(), length_ms: *ms, recording: Some(format!("{value}-rec-{position}")) }).collect();
        self.cat().keep_list(record, &Reference::new(source, kind, value), &list, 1).unwrap();
    }

    fn row(&self, record: i64, folder: i64) -> Row {
        which_release_row(&self.served, &ids::record(record), Some(&(folder, String::new()))).unwrap()
    }

    /// The copy in `folder`'s holding: whether it is whole, as the holding reads it (HD1, HD2).
    fn whole(&self, record: i64, folder: i64) -> bool {
        let kinds = self.served.library.kinds();
        let places = super::room::Places::of(&kinds, &self.cat().folders());
        let copies = self.cat().copies_of(&[record], &super::requests::library_of(&kinds, &places)).unwrap().remove(&record).unwrap_or_default();
        copies.copies.iter().find(|c| c.folder == folder).is_some_and(|c| cratarr_domain::holding::whole_of(c, copies.kept()))
    }
}

/// S1–S`n`, each its length (`ms`), as a list gives them.
fn salt(n: u32) -> Vec<(String, Option<i64>)> {
    (1..=n).map(|i| (format!("S{i}"), Some(ms(i)))).collect()
}

/// Bandcamp's ten: S1–S10, each 0.6 s longer (agreeing with MusicBrainz's within the slack).
fn salt_bandcamp() -> Vec<(String, Option<i64>)> {
    (1..=10).map(|i| (format!("S{i}"), Some(ms(i) + 600))).collect()
}

/// S1–S`n` as files, each its length.
fn salt_files(n: u32) -> Vec<(u32, String, i64)> {
    (1..=n).map(|i| (i, format!("S{i}"), ms(i))).collect()
}

fn by(row: &Row) -> Vec<&str> {
    row.release.by.iter().map(String::as_str).collect()
}

/// Salt Lines (F53's fixtures) with its three sure references: MusicBrainz's release read by its group, Bandcamp's page by
/// MusicBrainz's link, Discogs' release by its master.
fn salt_lines(fx: &Fx) -> i64 {
    salt_lines_as(fx, "")
}

/// Salt Lines as another record of its shape: its references' values end in `also` (a release is one record's alone).
fn salt_lines_as(fx: &Fx, also: &str) -> i64 {
    let r = fx.record(&format!("Salt Lines{also}"));
    fx.refer(r, "musicbrainz", "release", &format!("R70{also}"), "group");
    fx.refer(r, "bandcamp", "album", &format!("P70{also}"), "link");
    fx.refer(r, "discogs", "release", &format!("D70{also}"), "link");
    r
}

/// Salt Lines' three lists kept for the record `salt_lines_as(also)` made.
fn salt_kept(fx: &Fx, r: i64, also: &str) {
    fx.keep(r, "musicbrainz", "release", &format!("R70{also}"), &salt(10));
    fx.keep(r, "bandcamp", "album", &format!("P70{also}"), &salt_bandcamp());
    fx.keep(r, "discogs", "release", &format!("D70{also}"), &salt(12));
}

/// F53.3: every source named, in whatever order they answered. Salt Lines, IN2 holding S1–S10 (and its twelve-track twin,
/// S1–S12); three runs of one fixture, the lists kept MusicBrainz, Bandcamp, Discogs; Discogs, Bandcamp, MusicBrainz; Bandcamp,
/// Discogs, MusicBrainz. Once all three are kept the row is the same in every run, green by MusicBrainz and Bandcamp; while only
/// Discogs' twelve are kept, Closest to them with S11 and S12 missing, MusicBrainz and Bandcamp named as waiting; the twin whole by
/// Discogs' twelve alone, by V10 with S11 and S12 extra, then whole by both with no version named. Never two runs differing at the
/// end; never the first answer named alone. Red first: today's row reads MusicBrainz's match only.
#[tokio::test]
async fn the_which_release_row_reads_every_kept_list() {
    let _state = super::references::TestState::set().await;
    let mut ends = Vec::new();
    for order in [["musicbrainz", "bandcamp", "discogs"], ["discogs", "bandcamp", "musicbrainz"], ["bandcamp", "discogs", "musicbrainz"]] {
        let fx = Fx::new();
        let (salt_r, twin) = (salt_lines(&fx), salt_lines_twin(&fx));
        fx.copy(salt_r, IN2, &salt_files(10));
        fx.copy(twin, IN2, &salt_files(12));
        let mut kept: Vec<&str> = Vec::new();
        for source in order {
            for (r, also) in [(salt_r, ""), (twin, "t")] {
                match source {
                    "musicbrainz" => fx.keep(r, "musicbrainz", "release", &format!("R70{also}"), &salt(10)),
                    "bandcamp" => fx.keep(r, "bandcamp", "album", &format!("P70{also}"), &salt_bandcamp()),
                    _ => fx.keep(r, "discogs", "release", &format!("D70{also}"), &salt(12)),
                }
            }
            kept.push(source);
            let (row, twin_row) = (fx.row(salt_r, IN2), fx.row(twin, IN2));
            match kept.as_slice() {
                ["discogs"] => {
                    assert_eq!((row.release.state, by(&row)), (State::Closest, vec!["discogs"]), "{order:?}: {row:?}");
                    let missing: Vec<&str> = row.release.differ.iter().filter(|d| d.what == What::Missing).map(|d| d.title.as_str()).collect();
                    assert_eq!(missing, ["S11", "S12"]);
                    assert_eq!(row.release.waiting, ["musicbrainz", "bandcamp"], "the others named as waiting: {}", row.check.said);
                    assert!(row.check.said.contains("MusicBrainz and Bandcamp not read yet"), "{}", row.check.said);
                    assert_eq!((twin_row.release.state, by(&twin_row)), (State::Agrees, vec!["discogs"]));
                    assert_eq!(twin_row.check.said, "Matches Discogs: all 12 tracks; MusicBrainz and Bandcamp not read yet");
                }
                [_] if !kept.contains(&"discogs") => {
                    assert_eq!(row.release.state, State::Agrees, "{order:?}");
                    assert_eq!((twin_row.release.state, twin_row.release.version.is_some()), (State::Agrees, true), "whole by V10: {twin_row:?}");
                    let extra: Vec<&str> = twin_row.release.differ.iter().filter(|d| d.what == What::Extra).map(|d| d.title.as_str()).collect();
                    assert_eq!(extra, ["S11", "S12"], "S11 and S12 extra, green");
                }
                _ => {}
            }
        }
        let (row, twin_row) = (fx.row(salt_r, IN2), fx.row(twin, IN2));
        assert_eq!((row.check.state, by(&row)), ("green", vec!["musicbrainz", "bandcamp"]), "{order:?}");
        assert_eq!(row.check.said, "Matches MusicBrainz and Bandcamp: all 10 tracks");
        assert_eq!((twin_row.release.state, twin_row.release.version, twin_row.release.by.len()), (State::Agrees, None, 0), "the twin: whole, no version named");
        ends.push((row.check.clone(), twin_row.check.clone()));
    }
    assert!(ends.windows(2).all(|w| w[0] == w[1]), "one row in every run: {ends:?}");
}

/// The twin of Salt Lines (its twelve-track copy), on a record of its own, its references its own.
fn salt_lines_twin(fx: &Fx) -> i64 {
    salt_lines_as(fx, "t")
}

/// F53.2: Kelpe's "Answered EP", a Soulseek arrival on its request's record (the join WM.9 makes, a pin), names Bandcamp. al-140
/// holds its Bandcamp page by a link and its list of five; IN2 holds the five. The row green, "Matches Bandcamp: all 5 tracks", by
/// Bandcamp alone; each track line Bandcamp's length; with every other check green, Keep suggested. Never "No source knows it";
/// never a search for it (its reference is sure); never the read asking anything. Red first: today's row reads red.
#[tokio::test]
async fn kelpes_arrival_names_bandcamp() {
    use super::references_door::StandIn;
    let _state = super::references::TestState::set().await;
    let fx = Fx::new();
    let kelpe = fx.record("Answered EP");
    fx.refer(kelpe, "bandcamp", "album", "https://kelpe.bandcamp.com/album/answered-ep", "link");
    let five: Vec<(String, Option<i64>)> = (1..=5).map(|i| (format!("K{i}"), Some(ms(i)))).collect();
    fx.keep(kelpe, "bandcamp", "album", "https://kelpe.bandcamp.com/album/answered-ep", &five);
    fx.copy(kelpe, IN2, &(1..=5).map(|i| (i, format!("K{i}"), ms(i) + 400)).collect::<Vec<_>>());
    let row = fx.row(kelpe, IN2);
    assert_eq!((row.check.state, row.check.said.as_str(), by(&row)), ("green", "Matches Bandcamp: all 5 tracks", vec!["bandcamp"]), "{row:?}");
    for i in 0..5 {
        let (len, said) = track_says(&row.release, i).expect("each track line its reference length");
        assert!(said.starts_with("Bandcamp says") && len == ms(i as u32 + 1), "{said}");
    }
    let green = |key| Check { key, state: "green", said: String::new() };
    assert_eq!(suggested(&[row.check.clone(), green("acoustid"), green("sound"), green("one_record")]), Some("keep"));
    let door = StandIn::new();
    let door = Arc::new(door);
    assert!(super::arrival_job::begin_through(fx.served.clone(), kelpe, door.clone()).await.is_none(), "its reference is sure: no search");
    assert!(door.asked().is_empty(), "nothing asked for it");
}

/// F53.4 (the card): each track line says its version's sources. Salt Lines, S10 10:38 in every list, IN2's S10 2:00: Closest,
/// "MusicBrainz and Bandcamp, 10 tracks; track 10's length differs (2:00 against 10:38)", each line "MusicBrainz, Bandcamp say …",
/// S10's none. Tide Book's "T10" (3:11) agrees with Discogs' "T10 (Edit)" (3:10): its lines say Discogs. Never a line saying
/// MusicBrainz where MusicBrainz is not in the version named. Red first: today's lines say MusicBrainz's match.
#[tokio::test]
async fn a_track_line_says_its_versions_sources() {
    let _state = super::references::TestState::set().await;
    let fx = Fx::new();
    let r = salt_lines(&fx);
    let long = |n: u32| -> Vec<(String, Option<i64>)> { (1..=n).map(|i| (format!("S{i}"), Some(if i == 10 { 638_000 } else { ms(i) }))).collect() };
    fx.keep(r, "musicbrainz", "release", "R70", &long(10));
    fx.keep(r, "bandcamp", "album", "P70", &long(10));
    fx.keep(r, "discogs", "release", "D70", &long(12));
    let mut files = salt_files(10);
    files[9].2 = 120_000;
    fx.copy(r, IN2, &files);
    let row = fx.row(r, IN2);
    assert_eq!(row.check.said, "Closest: MusicBrainz and Bandcamp, 10 tracks; track 10's length differs (2:00 against 10:38)");
    let lines: Vec<Option<String>> = (0..10).map(|i| track_says(&row.release, i).map(|(_, s)| s)).collect();
    assert!(lines[..9].iter().all(|l| l.as_deref().is_some_and(|l| l.starts_with("MusicBrainz, Bandcamp say"))), "{lines:?}");
    assert_eq!(lines[9], None, "track 10's file pairs none");
    // Tide Book: MusicBrainz's T10 7:40, Discogs' "T10 (Edit)" 3:10, the file "T10" 3:11.
    let tide = fx.record("Tide Book");
    fx.refer(tide, "musicbrainz", "release", "R71", "link");
    fx.refer(tide, "discogs", "release", "D71", "link");
    fx.keep(tide, "musicbrainz", "release", "R71", &(1..=10).map(|i| (format!("T{i}"), Some(if i == 10 { 460_000 } else { ms(i) }))).collect::<Vec<_>>());
    fx.keep(tide, "discogs", "release", "D71", &(1..=10).map(|i| if i == 10 { ("T10 (Edit)".to_string(), Some(190_000)) } else { (format!("T{i}"), Some(ms(i))) }).collect::<Vec<_>>());
    fx.copy(tide, IN2, &(1..=10).map(|i| if i == 10 { (10, "T10".to_string(), 191_000) } else { (i, format!("T{i}"), ms(i)) }).collect::<Vec<_>>());
    let row = fx.row(tide, IN2);
    assert_eq!((row.check.said.as_str(), by(&row)), ("Matches Discogs: all 10 tracks", vec!["discogs"]));
    assert!((0..10).all(|i| track_says(&row.release, i).is_some_and(|(_, s)| s.starts_with("Discogs says"))), "never MusicBrainz's");
}

/// F53.5 (the card's words): Grey Shore's two MusicBrainz releases, R141a (2013) and R141b (2015, G3 6 s longer), both kept; IN2
/// holds R141b's G3. "Matches MusicBrainz's 2015 release: all 10 tracks". Never R141a for being read first.
#[tokio::test]
async fn the_row_names_the_release_it_matches_on_the_card() {
    let _state = super::references::TestState::set().await;
    let fx = Fx::new();
    let grey = fx.record("Grey Shore");
    for (value, g3, year) in [("R141a", ms(3), "2013"), ("R141b", ms(3) + 6_000, "2015")] {
        fx.refer(grey, "musicbrainz", "release", value, "link");
        fx.keep(grey, "musicbrainz", "release", value, &(1..=10).map(|i| (format!("G{i}"), Some(if i == 3 { g3 } else { ms(i) }))).collect::<Vec<_>>());
        fx.cat().said(grey, "year", "musicbrainz", &crate::catalogue::facts::of_ref("release", value), Some(year), 1).unwrap();
    }
    fx.copy(grey, IN2, &(1..=10).map(|i| (i, format!("G{i}"), if i == 3 { ms(3) + 6_000 } else { ms(i) })).collect::<Vec<_>>());
    let row = fx.row(grey, IN2);
    assert_eq!(row.check.said, "Matches MusicBrainz's 2015 release: all 10 tracks");
    assert_eq!(row.release.releases.iter().map(|r| r.value.as_str()).collect::<Vec<_>>(), ["R141b"]);
}

/// F53.8: red only where no source references the record. (a) al-142, only Last.fm's kept words: no list; (b) Salt Lines, every
/// source switched off since its lists were kept: the kept lists judge; (c) its references kept, no list read, MusicBrainz switched
/// off: waiting on Discogs and Bandcamp; (d) al-143, no reference, its searches answered with nothing: red, the only red; (e) its
/// two guesses, neither pairing whole: amber, both named. Never red on (a), (b), (c), (e); never the read storing or asking anything.
/// Red first: today's row reads red without MusicBrainz's match or Discogs' facts.
#[tokio::test]
async fn red_only_where_no_source_references_it() {
    use crate::app::switches::{BANDCAMP, DISCOGS, MUSICBRAINZ, set_for_tests};
    let _state = super::references::TestState::set().await;
    let fx = Fx::new();
    // (a) Last.fm's words alone.
    let low = fx.record("Low Fields");
    let words = [crate::userdata::words::RawWord { word: "idm".into(), kind: "tag".into(), weight: 1.0 }, crate::userdata::words::RawWord { word: "ambient".into(), kind: "tag".into(), weight: 0.8 }];
    fx.served.userdata.set_words(&ids::record(low), "lastfm", &words, true, 1).unwrap();
    fx.copy(low, IN2, &salt_files(10));
    let a = fx.row(low, IN2);
    assert_eq!((a.release.state, a.check.state, a.check.said.as_str()), (State::NoList, "amber", "No source gives its tracklist"));
    // (b) Every source switched off since its lists were kept.
    let salt_r = salt_lines(&fx);
    fx.keep(salt_r, "musicbrainz", "release", "R70", &salt(10));
    fx.keep(salt_r, "bandcamp", "album", "P70", &salt_bandcamp());
    fx.keep(salt_r, "discogs", "release", "D70", &salt(12));
    fx.copy(salt_r, IN2, &salt_files(10));
    let was: Vec<(&'static str, bool)> = [MUSICBRAINZ, DISCOGS, BANDCAMP].into_iter().map(|s| (s, set_for_tests(s, false))).collect();
    let b = fx.row(salt_r, IN2);
    assert_eq!((b.check.state, by(&b)), ("green", vec!["musicbrainz", "bandcamp"]), "the kept lists judge: {b:?}");
    for (s, on) in was {
        set_for_tests(s, on);
    }
    // (c) Its references kept, no list read; MusicBrainz switched off, Discogs and Bandcamp not asked.
    let unread = salt_lines_twin(&fx);
    fx.copy(unread, IN2, &salt_files(10));
    let was = set_for_tests(MUSICBRAINZ, false);
    let c = fx.row(unread, IN2);
    set_for_tests(MUSICBRAINZ, was);
    assert_eq!((c.release.state, c.release.waiting.clone()), (State::Waiting, vec!["discogs".to_string(), "bandcamp".to_string()]), "{c:?}");
    // (d) No reference at all, its searches answered with nothing.
    let none = fx.record("Untitled Shore");
    fx.copy(none, IN2, &salt_files(10));
    for source in ["musicbrainz", "discogs"] {
        fx.served.userdata.set_import_choice(&format!("arrival:searched:{source}:{}", ids::record(none)), "1").unwrap();
    }
    let d = fx.row(none, IN2);
    assert_eq!((d.check.state, d.check.said.as_str()), ("red", "No source knows it"));
    // (e) Two guesses, neither pairing whole.
    let guessed = fx.record("Untitled Shore (guessed)");
    fx.copy(guessed, IN2, &salt_files(10));
    fx.refer(guessed, "musicbrainz", "release", "R143", "search");
    fx.keep(guessed, "musicbrainz", "release", "R143", &(1..=10).map(|i| (format!("S{i}"), Some(if i == 7 { ms(7) + 6_000 } else { ms(i) }))).collect::<Vec<_>>());
    fx.refer(guessed, "discogs", "master", "D143", "search");
    fx.keep(guessed, "discogs", "master", "D143", &(1..=10).map(|i| (format!("S{i}"), None)).collect::<Vec<_>>());
    let e = fx.row(guessed, IN2);
    assert_eq!((e.release.state, e.check.state), (State::Guessed, "amber"), "{e:?}");
    assert_eq!(e.release.guesses.iter().map(|g| g.reference.value.as_str()).collect::<Vec<_>>(), ["R143", "D143"]);
    for row in [&a, &b, &c, &e] {
        assert_ne!(row.check.state, "red");
    }
}

/// F53.8 (f): 0.17's carried certain match, judged by today's pairing (the owner's 02:32): al-145's R145, `how` "search", never
/// confirmed. (f1) its list not read: a guess; (f2) MUSIC's copy pairs whole with its list both ways: a reference, the IN2 arrival
/// green by it; (f3) MUSIC's track 4 6 s longer: a guess; (f4) only the arrival pairs whole: a guess (C15), no count shown. Never
/// the stored "certain" label read; never the arrival confirming by its own pairing. Red first: today's row reads the stored match.
#[tokio::test]
async fn a_carried_match_is_judged_by_todays_pairing() {
    let _state = super::references::TestState::set().await;
    let field = |fx: &Fx| -> i64 {
        let r = fx.record("Field Notes");
        fx.refer(r, "musicbrainz", "release", "R145", "search");
        fx.copy(r, IN2, &salt_files(10));
        r
    };
    // (f1)
    let fx = Fx::new();
    let r = field(&fx);
    let f1 = fx.row(r, IN2);
    assert_eq!((f1.release.state, f1.check.state), (State::Guessed, "amber"));
    assert_eq!(f1.release.guesses.iter().map(|g| g.reference.value.as_str()).collect::<Vec<_>>(), ["R145"]);
    // (f2)
    let fx = Fx::new();
    let r = field(&fx);
    fx.keep(r, "musicbrainz", "release", "R145", &salt(10));
    fx.copy(r, MUSIC, &(1..=10).map(|i| (i, format!("S{i}"), ms(i) + 500)).collect::<Vec<_>>());
    let f2 = fx.row(r, IN2);
    assert_eq!((f2.check.state, f2.check.said.as_str()), ("green", "Matches MusicBrainz: all 10 tracks"), "{f2:?}");
    // (f3)
    let fx = Fx::new();
    let r = field(&fx);
    fx.keep(r, "musicbrainz", "release", "R145", &salt(10));
    fx.copy(r, MUSIC, &(1..=10).map(|i| (i, format!("S{i}"), if i == 4 { ms(4) + 6_000 } else { ms(i) })).collect::<Vec<_>>());
    let f3 = fx.row(r, IN2);
    assert_eq!((f3.release.state, f3.check.state), (State::Guessed, "amber"));
    // (f4)
    let fx = Fx::new();
    let r = field(&fx);
    fx.keep(r, "musicbrainz", "release", "R145", &salt(10));
    let f4 = fx.row(r, IN2);
    assert_eq!((f4.release.state, f4.check.state), (State::Guessed, "amber"), "the arrival confirms nothing (C15): {f4:?}");
    assert!(!fx.whole(r, IN2), "a guess never makes the copy whole: no count shows");
}

/// F53.9 (kind 3): a busy source never holds the row. Salt Lines, IN2 holding S1–S10, Bandcamp's ten kept, MusicBrainz answering
/// 503 (its turn held) and Discogs not asked: green by Bandcamp, MusicBrainz named as not answering; Discogs' twelve kept after:
/// unchanged; MusicBrainz's ten after it answers: by MusicBrainz and Bandcamp. Each change logged for the open section (LV1). Never
/// the whole row waiting on MusicBrainz; never the read asking it. Red first: today's row waits on MusicBrainz.
#[tokio::test]
async fn a_busy_source_never_holds_the_row() {
    use crate::app::reach::stand_ins::StandIns;
    let _state = super::references::TestState::set().await;
    let stand_ins = StandIns::set(Arc::new(|_: &crate::app::reach::stand_ins::Asked| None)).await;
    let fx = Fx::new();
    let r = salt_lines(&fx);
    fx.copy(r, IN2, &salt_files(10));
    fx.keep(r, "bandcamp", "album", "P70", &salt_bandcamp());
    let held = crate::app::musicbrainz::TURN.as_held();
    crate::app::musicbrainz::TURN.hold_until(crate::app::moment::now_secs() + 600, None);
    let logged = |before: i64| {
        fx.served.refresh().unwrap();
        let now = fx.served.userdata.change_rev().unwrap();
        assert!(now > before, "the change reaches the open section (LV1): {now} > {before}");
        now
    };
    let rev = fx.served.userdata.change_rev().unwrap();
    let t = fx.row(r, IN2);
    assert_eq!((t.check.state, by(&t)), ("green", vec!["bandcamp"]), "{t:?}");
    assert!(t.check.said.starts_with("Matches Bandcamp: all 10 tracks") && t.check.said.contains("MusicBrainz didn't answer; asked again later"), "{}", t.check.said);
    fx.keep(r, "discogs", "release", "D70", &salt(12));
    let rev = logged(rev);
    let t10 = fx.row(r, IN2);
    assert_eq!((t10.check.state, by(&t10)), ("green", vec!["bandcamp"]), "Discogs' twelve differ and are not named");
    crate::app::musicbrainz::TURN.put_back(held);
    fx.keep(r, "musicbrainz", "release", "R70", &salt(10));
    logged(rev);
    let after = fx.row(r, IN2);
    assert_eq!(after.check.said, "Matches MusicBrainz and Bandcamp: all 10 tracks");
    assert!(stand_ins.asked().is_empty(), "the reads asked nothing: {:?}", stand_ins.asked());
}

/// F53.10 (kind 6): the row, the holding and Versions read one rule. Every copy of F53.1 (a)–(i) and F53.6 (with the pick) on its
/// record in IN2: the row Agrees exactly where the holding reads the copy whole; its `by` the version's own sources; never "has of"
/// counts said from the named version. Red first: on F53.1 (b)'s copy, today's row is amber while the holding reads it whole.
#[tokio::test]
async fn the_row_and_the_holding_agree() {
    let _state = super::references::TestState::set().await;
    let fx = Fx::new();
    let low = |ns: &[u32]| -> Vec<(String, Option<i64>)> { ns.iter().map(|n| (format!("L{n}"), Some(ms(*n)))).collect() };
    let low_files = |ns: &[u32]| -> Vec<(u32, String, i64)> { ns.iter().zip(1..).map(|(n, place)| (place, format!("L{n}"), ms(*n))).collect() };
    let mut cases: Vec<(&str, i64, Option<Reference>)> = Vec::new();
    for (name, files) in [("a", salt_files(10)), ("b", salt_files(12)), ("d", salt_files(9))] {
        let r = salt_lines_as(&fx, name);
        salt_kept(&fx, r, name);
        fx.copy(r, IN2, &files);
        cases.push((name, r, None));
    }
    // (c) S7 5 s longer; (e) "S4 (Bonus Track)"; (f) "S10 (Live)".
    for (name, n, title, delta) in [("c", 7u32, "S7", 5_000i64), ("e", 4, "S4 (Bonus Track)", 0), ("f", 10, "S10 (Live)", 900)] {
        let r = salt_lines_as(&fx, name);
        salt_kept(&fx, r, name);
        let mut files = salt_files(10);
        files[n as usize - 1] = (n, title.to_string(), ms(n) + delta);
        fx.copy(r, IN2, &files);
        cases.push((name, r, None));
    }
    // (g) V10 alone kept, S1–S12.
    let g = fx.record("Salt Lines (g)");
    fx.refer(g, "musicbrainz", "release", "R70g", "group");
    fx.keep(g, "musicbrainz", "release", "R70g", &salt(10));
    fx.copy(g, IN2, &salt_files(12));
    cases.push(("g", g, None));
    // (h), (i) Low Ceiling.
    for (name, ns) in [("h", vec![1, 2, 3, 4, 5, 6, 7, 8, 10, 11]), ("i", vec![1, 2, 3, 4, 5, 6, 7, 8, 10])] {
        let r = fx.record("Low Ceiling");
        fx.refer(r, "musicbrainz", "release", &format!("R72{name}"), "link");
        fx.refer(r, "discogs", "release", &format!("D72{name}"), "link");
        fx.keep(r, "musicbrainz", "release", &format!("R72{name}"), &low(&[1, 2, 3, 4, 5, 6, 7, 8, 9]));
        fx.keep(r, "discogs", "release", &format!("D72{name}"), &low(&[1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12]));
        fx.copy(r, IN2, &low_files(&ns));
        cases.push((name, r, None));
    }
    // F53.6: root's pick of Discogs' twelve, (a) S1–S12, (b) S1–S10.
    for (name, n) in [("6a", 12), ("6b", 10)] {
        let r = fx.record("Salt Lines (picked)");
        let value = format!("D70{name}");
        fx.refer(r, "musicbrainz", "release", &format!("R70{name}"), "group");
        fx.refer(r, "discogs", "release", &value, "link");
        fx.keep(r, "musicbrainz", "release", &format!("R70{name}"), &salt(10));
        fx.keep(r, "discogs", "release", &value, &salt(12));
        let pick = Reference::new("discogs", "release", &value);
        assert!(fx.cat().pick_list(r, Some(&pick)).unwrap());
        fx.copy(r, IN2, &salt_files(n));
        cases.push((name, r, Some(pick)));
    }
    for (name, r, pick) in cases {
        let row = fx.row(r, IN2);
        let whole = fx.whole(r, IN2);
        assert_eq!(row.release.state == State::Agrees, whole, "{name}: the row Agrees exactly where the copy is whole: {row:?}");
        if !whole {
            assert_eq!(row.release.state, State::Closest, "{name}: Closest exactly where it is not");
        }
        if let Some(i) = row.release.version {
            assert_eq!(row.release.by, row.versions[i].by, "{name}: the version's own sources");
        }
        if pick.is_some() {
            assert!(row.release.picked || row.release.version.is_none(), "{name}: the pick judges");
        }
        assert!(!row.check.said.contains(" of "), "{name}: no count of the named version said as the holding's: {}", row.check.said);
    }
}

/// F53.12: a Lidarr arrival named by its files' release, every source of it compared. al-144's files name R144 (sure); MusicBrainz's
/// link names D144 and P144; MusicBrainz's eleven and Bandcamp's agree, Discogs' eleven without lengths agree by titles: "Matches
/// MusicBrainz, Discogs and Bandcamp: all 11 tracks". C12: a file 6 s off stays a length difference though every title agrees. Never
/// a search for it. Red first: today's row names "a MusicBrainz edition" alone.
#[tokio::test]
async fn a_files_release_arrival_names_every_agreeing_source() {
    use super::references_door::StandIn;
    let _state = super::references::TestState::set().await;
    let night = |n: u32| -> Vec<(String, Option<i64>)> { (1..=n).map(|i| (format!("N{i}"), Some(ms(i)))).collect() };
    let build = |fx: &Fx, off: Option<u32>| -> i64 {
        let r = fx.record("Night Ferry II");
        fx.refer(r, "musicbrainz", "release", "R144", "files");
        fx.refer(r, "discogs", "release", "D144", "link");
        fx.refer(r, "bandcamp", "album", "P144", "link");
        fx.keep(r, "musicbrainz", "release", "R144", &night(11));
        fx.keep(r, "bandcamp", "album", "P144", &(1..=11).map(|i| (format!("N{i}"), Some(ms(i) + 700))).collect::<Vec<_>>());
        fx.keep(r, "discogs", "release", "D144", &(1..=11).map(|i| (format!("N{i}"), None)).collect::<Vec<_>>());
        fx.copy(r, IN2, &(1..=11).map(|i| (i, format!("N{i}"), if Some(i) == off { ms(i) + 6_000 } else { ms(i) })).collect::<Vec<_>>());
        r
    };
    let fx = Fx::new();
    let r = build(&fx, None);
    let row = fx.row(r, IN2);
    assert_eq!((row.check.state, row.check.said.as_str()), ("green", "Matches MusicBrainz, Discogs and Bandcamp: all 11 tracks"), "{row:?}");
    let door = Arc::new(StandIn::new());
    assert!(super::arrival_job::begin_through(fx.served.clone(), r, door.clone()).await.is_none());
    assert!(door.asked().is_empty(), "its files name its release: no search");
    let fx = Fx::new();
    let off = build(&fx, Some(5));
    let row = fx.row(off, IN2);
    assert_eq!(row.release.state, State::Closest, "never whole by Discogs' lengthless list beside one with lengths (C12): {row:?}");
    assert!(matches!(row.release.differ.as_slice(), [d] if matches!(d.what, What::Length { .. })), "{row:?}");
}

/// The database as the reads may never change it: its references, lists, facts and the arrival's marks.
fn kept_rows(fx: &Fx, record: i64) -> (Vec<String>, usize, usize, usize, Option<String>) {
    let refs: Vec<String> = fx.cat().references_of(&ids::record(record)).unwrap().into_iter().map(|(r, row)| format!("{}:{}:{} {} {}", r.source, r.kind, r.value, row.how, row.confirmed)).collect();
    let lists = fx.cat().lists_kept(record).unwrap().len();
    let facts = fx.cat().facts_said(record).unwrap().len();
    let marks = fx.served.userdata.import_choices("arrival:").unwrap().len();
    let counting = fx.cat().record(record).unwrap().unwrap().counting;
    (refs, lists, facts, marks, counting)
}

/// `al-143`'s shape: made by an arrival's tags in IN2 (a folder dropped in by hand: no item, no ids), no reference, ten files.
fn untitled(fx: &Fx) -> i64 {
    let r = fx.record("Untitled Shore");
    fx.copy(r, IN2, &salt_files(10));
    r
}

/// The stand-in door F53.7 (b) asks through: MusicBrainz's search finding R143 (ten tracks with lengths, each within 1 s of the
/// files; track 7 `off` ms longer where given), Discogs' finding D143 (ten tracks, no lengths, the titles agreeing). It answers
/// nothing of Bandcamp's: a question of it is a violation.
fn door_for(off: i64) -> Arc<super::references_door::StandIn> {
    use super::references_door::{About, Candidate, Listing, StandIn};
    let mb: Vec<RefTrack> = (1..=10).map(|i| RefTrack { disc: 1, position: i, title: format!("S{i}"), length_ms: Some(ms(i) + 300 + if i == 7 { off } else { 0 }), recording: Some(format!("rec-{i}")) }).collect();
    let dc: Vec<RefTrack> = (1..=10).map(|i| RefTrack { disc: 1, position: i, title: format!("S{i}"), length_ms: None, recording: None }).collect();
    let about = About { title: "Untitled Shore".into(), artist: "Glass Harbour".into(), year: Some(2019), ..About::default() };
    Arc::new(
        StandIn::new()
            .find("Glass Harbour", "Untitled Shore", vec![Candidate { id: "R143".into(), score: 100, tracks: Some(10) }])
            .masters("Glass Harbour", "Untitled Shore", vec![Candidate { id: "D143".into(), score: 100, tracks: None }])
            .tracklist(Ref::new("musicbrainz", "release", "R143"), Some(Listing { tracks: mb, about: Some(about), ..Listing::default() }))
            .tracklist(Ref::new("discogs", "master", "D143"), Some(Listing { tracks: dc, ..Listing::default() })),
    )
}

/// Runs the arrival's job for `record` through `door` to its end (begun as the record pass begins it, no one placing it).
async fn job(fx: &Fx, record: i64, door: Arc<super::references_door::StandIn>) {
    let task = super::arrival_job::begin_through(fx.served.clone(), record, door).await.expect("the arrival's job begun");
    let id = format!("job:{}", task.id);
    for _ in 0..1000 {
        if crate::runner::runner().rows().iter().all(|r| r.id != id) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("the arrival's job never ended");
}

/// Whether a job of the arrival's or the hearing's kind runs now (the reads begin none).
fn arrival_jobs_running() -> bool {
    crate::runner::runner().rows().iter().any(|r| r.kind == super::arrival_job::KIND || r.kind == super::intake_card::KIND)
}

/// F53.7 (b): opening the arrival makes nothing. al-143, no reference; the section read before the job's searches answer, after, and
/// once more: no source asked (every stand-in counts zero from the reads), no task begun, nothing stored. While the searches haven't
/// answered the row waits on them, never red. The job (begun by the record pass, no one placing it) asks MusicBrainz's search and
/// Discogs' once each, never Bandcamp, keeps each candidate (`how` "search", not confirmed) and its list, no "certain" label and no
/// count. Red first: today's read searches MusicBrainz and stores its match.
#[tokio::test]
async fn opening_the_arrival_makes_nothing() {
    use crate::app::reach::stand_ins::StandIns;
    use super::references_door::Asked;
    let _state = super::references::TestState::set().await;
    let stand_ins = StandIns::set(Arc::new(|_: &crate::app::reach::stand_ins::Asked| None)).await;
    let fx = Fx::new();
    let r = untitled(&fx);
    let before = kept_rows(&fx, r);
    let first = fx.row(r, IN2);
    assert_eq!((first.check.state, first.release.waiting.clone()), ("waiting", vec!["musicbrainz".to_string(), "discogs".to_string()]), "waits on the searches, never red: {first:?}");
    assert_eq!(kept_rows(&fx, r), before, "the read stored nothing");
    assert!(!arrival_jobs_running(), "the read began nothing");
    let door = door_for(0);
    job(&fx, r, door.clone()).await;
    let asked = door.asked();
    assert_eq!(asked.iter().filter(|a| matches!(a, Asked::Find(..))).count(), 1, "MusicBrainz's search once: {asked:?}");
    assert_eq!(asked.iter().filter(|a| matches!(a, Asked::FindMasters(..))).count(), 1, "Discogs' once");
    assert!(asked.iter().all(|a| !matches!(a, Asked::Tracklist(t) if t.source == "bandcamp")), "never Bandcamp");
    assert!(door.violations().is_empty(), "{:?}", door.violations());
    let (refs, lists, _, _, counting) = kept_rows(&fx, r);
    assert_eq!(refs, ["discogs:master:D143 search false", "musicbrainz:release:R143 search false"], "each candidate a guess");
    assert_eq!((lists, counting), (2, None), "each list kept; no count worked out");
    let after = kept_rows(&fx, r);
    for _ in 0..2 {
        let _ = fx.row(r, IN2);
        assert_eq!(kept_rows(&fx, r), after, "the reads after store nothing");
    }
    assert!(stand_ins.asked().is_empty(), "no outside ask from the reads or past the door: {:?}", stand_ins.asked());
    // Asked once: begun again, nothing more.
    assert!(super::arrival_job::begin_through(fx.served.clone(), r, door.clone()).await.is_none(), "each search once");
}

/// F53.7 (b), C15: an arrival never confirms its own find. After the job, the arrival's pairing with MusicBrainz's find is whole, but
/// an Incoming arrival isn't added yet: both finds stay guesses, the row amber, the copy not whole (no count, no delivery). Red first:
/// today's row is green by the stored "certain" label.
#[tokio::test]
async fn an_arrival_never_confirms_its_own_find() {
    let _state = super::references::TestState::set().await;
    let fx = Fx::new();
    let r = untitled(&fx);
    job(&fx, r, door_for(0)).await;
    let row = fx.row(r, IN2);
    assert_eq!((row.release.state, row.check.state), (State::Guessed, "amber"), "{row:?}");
    assert!(!fx.whole(r, IN2), "a guess never makes the copy whole");
}

/// F53.7 (b), (e): the guesses shown as guesses, MusicBrainz's and Discogs' alike, each "probably", with Confirm; Keep not suggested.
/// (b) Confirm of MusicBrainz's: its list judges, green, Discogs' guess no longer shown, Keep suggested with the others green. (e) its
/// track 7 6 s longer: Confirm makes it Closest, track 7's length said, amber. Red first: no Discogs candidate beside MusicBrainz's,
/// no Confirm.
#[tokio::test]
async fn a_guess_is_named_as_a_guess() {
    let _state = super::references::TestState::set().await;
    let green = |key| Check { key, state: "green", said: String::new() };
    for (off, after) in [(0i64, State::Agrees), (6_000, State::Closest)] {
        let fx = Fx::new();
        let r = untitled(&fx);
        job(&fx, r, door_for(off)).await;
        let row = fx.row(r, IN2);
        assert_eq!(row.release.state, State::Guessed);
        assert_eq!(row.release.guesses.iter().map(|g| (g.reference.source.as_str(), g.reference.value.as_str())).collect::<Vec<_>>(), [("musicbrainz", "R143"), ("discogs", "D143")]);
        assert!(row.check.said.starts_with("Probably MusicBrainz's Untitled Shore or Discogs' D143"), "{}", row.check.said);
        assert_eq!(suggested(&[row.check.clone(), green("acoustid")]), None, "Keep not suggested while the row is a guess");
        let mb = Reference::new("musicbrainz", "release", "R143");
        assert_eq!(confirm(&fx.served, &ids::record(r), &mb).unwrap(), Confirmed::Kept);
        let row = fx.row(r, IN2);
        assert_eq!((row.release.state, by(&row), row.release.guesses.len()), (after, vec!["musicbrainz"], 0), "{off}: {row:?}");
        if after == State::Agrees {
            assert_eq!(row.check.said, "Matches MusicBrainz: all 10 tracks");
            assert_eq!(suggested(&[row.check.clone(), green("acoustid")]), Some("keep"));
        } else {
            assert!(matches!(row.release.differ.as_slice(), [d] if matches!(d.what, What::Length { .. }) && d.title == "S7"), "{row:?}");
        }
    }
}

/// F53.7 (f), C15: a main copy confirms a find. As (b), with al-143 also in MUSIC (ten files pairing whole with MusicBrainz's find,
/// each within 1 s): MusicBrainz's find is a reference by that pairing, worked out at the read, no Confirm; the arrival's row green
/// by it. Red first: today's green comes from the stored label, not MUSIC's copy.
#[tokio::test]
async fn a_main_copy_confirms_a_find() {
    let _state = super::references::TestState::set().await;
    let fx = Fx::new();
    let r = untitled(&fx);
    fx.copy(r, MUSIC, &(1..=10).map(|i| (i, format!("S{i}"), ms(i) + 600)).collect::<Vec<_>>());
    job(&fx, r, door_for(0)).await;
    let row = fx.row(r, IN2);
    assert_eq!((row.check.state, row.check.said.as_str()), ("green", "Matches MusicBrainz: all 10 tracks"), "{row:?}");
    let refs = fx.cat().references_of(&ids::record(r)).unwrap();
    assert!(refs.iter().all(|(_, row)| row.how == "search" && !row.confirmed), "nothing stored says so: {refs:?}");
}

/// F53.7 (d), kind 4: two Confirms race, one stands. (e) after the job; root confirms MusicBrainz's candidate, then dave, his section
/// read before root's Confirm, Discogs': refused "It changed since; nothing was undone.", nothing kept; the row shows root's
/// reference. Red first: no Confirm on the row.
#[tokio::test]
async fn two_confirms_race_one_stands() {
    let _state = super::references::TestState::set().await;
    let fx = Fx::new();
    let r = untitled(&fx);
    job(&fx, r, door_for(6_000)).await;
    let daves = fx.row(r, IN2);
    assert_eq!(daves.release.guesses.len(), 2, "dave's section shows both");
    assert_eq!(confirm(&fx.served, &ids::record(r), &Reference::new("musicbrainz", "release", "R143")).unwrap(), Confirmed::Kept);
    let kept = kept_rows(&fx, r);
    assert_eq!(confirm(&fx.served, &ids::record(r), &Reference::new("discogs", "master", "D143")).unwrap(), Confirmed::ChangedSince);
    assert_eq!(kept_rows(&fx, r), kept, "a refused Confirm keeps nothing");
    let row = fx.row(r, IN2);
    assert_eq!(by(&row), ["musicbrainz"], "root's confirmed reference: {row:?}");
}

/// F53.7 (a), a guard on VR01's per-reference lists: a Bandcamp page 0.16's search found by name (`how` "search"), its seven kept,
/// beside Salt Lines' sure lists, IN2 holding those seven: never named as agreeing, judging nothing; the row reads as without it, and
/// shows no guess (the record has sure lists).
#[tokio::test]
async fn a_guessed_list_is_no_source_of_the_row() {
    let _state = super::references::TestState::set().await;
    let fx = Fx::new();
    let r = salt_lines(&fx);
    fx.keep(r, "musicbrainz", "release", "R70", &salt(10));
    fx.keep(r, "bandcamp", "album", "P70", &salt_bandcamp());
    fx.keep(r, "discogs", "release", "D70", &salt(12));
    fx.refer(r, "bandcamp", "album", "P70s", "search");
    fx.keep(r, "bandcamp", "album", "P70s", &salt(7));
    fx.copy(r, IN2, &salt_files(7));
    let row = fx.row(r, IN2);
    assert_eq!((row.release.state, by(&row)), (State::Closest, vec!["musicbrainz", "bandcamp"]), "V10 and V12 judge: {row:?}");
    assert!(row.release.releases.iter().all(|x| x.value != "P70s") && row.release.guesses.is_empty(), "the guessed page names nothing");
}
