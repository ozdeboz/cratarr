//! The domain's tables, read whole (SCENARIOS-DC1 CN1; TY4). `decide`'s spec is the policy table: shared/policy.json holds
//! CN1's rows as the scenario writes them, the page's tests read the same file, and every row is asked of `decide` for every
//! viewer. The file is kept in its written form: a table edited by hand and not written again fails.

use std::collections::BTreeSet;

use cratarr_domain::arrival::Arrival;
use cratarr_domain::can::{Action as A, ArrivalFacts, Decision, May, Record, RecordCopy, Scope, Target, Viewer, Why, can, decide};
use cratarr_domain::holding::{CopyFacts, Fetcher, Holding, Library, On, holding};
use serde::{Deserialize, Serialize};

const POLICY: &str = include_str!("../../../../shared/policy.json");

// SCENARIOS-DC1's libraries, by their catalogue folders.
const MUSIC: i64 = 0;
const IN: i64 = 1;
const OWN_A: i64 = 2;
const OWN_B: i64 = 3;
const MORE: i64 = 4;
const IN2: i64 = 5;
const UPL: i64 = 6;

#[derive(Debug, Deserialize, Serialize)]
struct Table {
    about: String,
    viewers: Vec<String>,
    rows: Vec<Row>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Row {
    state: String,
    action: String,
    answers: Vec<String>,
}

/// The accounts: root (an administrator), the reading key (an API key root made, scope Read), alice and bob (users with
/// "Request albums", "Download in apps" and "Upload music", their own music OWN-A and OWN-B), carol (none of them).
fn viewer(name: &str) -> Viewer {
    let user = May { request: true, offline: true, playlists: true };
    match name {
        "root" => Viewer::new("root", true, None, May::default(), None),
        "key" => Viewer::new("root", true, Some(Scope::Read), May::default(), None),
        // Home Assistant's: an administrator's control key (CHECK-DC1's ruling 2, HA-CONTRACT-0.18).
        "control" => Viewer::new("root", true, Some(Scope::Control), May::default(), None),
        // Another administrator, signed in (ruling 3: any administrator undoes shared data).
        "admin2" => Viewer::new("admin2", true, None, May::default(), None),
        "alice" => Viewer::new("alice", false, None, user, Some(OWN_A)),
        "bob" => Viewer::new("bob", false, None, user, Some(OWN_B)),
        "carol" => Viewer::new("carol", false, None, May { request: false, offline: false, playlists: true }, None),
        other => panic!("a viewer the table does not know: {other}"),
    }
}

fn main_lib(folder: i64) -> Target {
    Target { folder, library: Library::Main }
}

fn own_lib(folder: i64, owner: &str) -> Target {
    Target { folder, library: Library::Own(owner.to_string()) }
}

fn incoming_lib(folder: i64, fetcher: Fetcher, uploader: Option<&str>) -> Target {
    Target { folder, library: Library::Incoming { fetcher, uploader: uploader.map(str::to_string) } }
}

fn copy(target: Target, added: i64) -> RecordCopy {
    RecordCopy { folder: target.folder, library: target.library, added, converting: false, arrival: None }
}

fn waiting(target: Target, asked_by: &[&str]) -> RecordCopy {
    let arrival = ArrivalFacts { state: Arrival::Waiting, asked_by: asked_by.iter().map(|a| a.to_string()).collect(), by: None };
    RecordCopy { arrival: Some(arrival), ..copy(target, 300) }
}

/// A record of these whole copies for one viewer, its holding by the rules (every library counted, as the requests read it).
fn record(copies: Vec<RecordCopy>, plays: Option<usize>, v: &Viewer) -> Record {
    let facts: Vec<CopyFacts> =
        copies.iter().map(|c| CopyFacts { folder: c.folder, library: c.library.clone(), present: 10, paired: 10, went: None }).collect();
    Record { holding: holding(&facts, Some(10), &v.account, &On::all()), copies, plays, preview: false, open_requests: Vec::new() }
}

/// Each state of the table, for one viewer.
fn fixture(state: &str, v: &Viewer) -> Record {
    let not_had = Record { copies: Vec::new(), plays: None, preview: false, holding: Holding::NotHad, open_requests: Vec::new() };
    match state {
        "not had, Bandcamp page P7" => Record { preview: true, ..not_had },
        "not had, alice's request open" => Record { open_requests: vec!["alice".to_string()], ..not_had },
        "had, MUSIC" => record(vec![copy(main_lib(MUSIC), 10)], Some(0), v),
        "had only in OWN-A, alice's first" => record(vec![copy(own_lib(OWN_A, "alice"), 10)], Some(0), v),
        "MUSIC's copy converting" => record(vec![RecordCopy { converting: true, ..copy(main_lib(MUSIC), 10) }], Some(0), v),
        // "Two Rooms": MUSIC holds an older MP3.
        "arrival in IN, asked by alice" => {
            record(vec![copy(main_lib(MUSIC), 10), waiting(incoming_lib(IN, Fetcher::Lidarr, None), &["alice"])], Some(0), v)
        }
        "arrival in IN2, asked by nobody" => record(vec![waiting(incoming_lib(IN2, Fetcher::Soulseek, None), &[])], None, v),
        "bob's upload in UPL, waiting" => record(vec![waiting(incoming_lib(UPL, Fetcher::Uploads, Some("bob")), &[])], None, v),
        other => panic!("a state the table does not know: {other}"),
    }
}

/// The arrival in IN after root's Add to MUSIC: its row's Undo.
fn kept_by_root(v: &Viewer) -> Record {
    let kept = ArrivalFacts { state: Arrival::Kept { into: MUSIC }, asked_by: vec!["alice".to_string()], by: Some("root".to_string()) };
    record(vec![copy(main_lib(MUSIC), 10), RecordCopy { arrival: Some(kept), ..copy(main_lib(MUSIC), 300) }], Some(0), v)
}

/// What each of the table's action cells asks, in its state.
fn actions(state: &str, action: &str) -> Vec<A> {
    match (state, action) {
        (_, "the preview's Play row") => vec![A::Play],
        // Play reads the record's tracks (a GET); the rest are queued or started on a player (a POST, never a reading key: ruling 1).
        (_, "Play") => vec![A::Play],
        (_, "Shuffle, Play next, Add to queue, Start radio") => vec![A::Shuffle, A::PlayNext, A::AddToQueue, A::StartRadio],
        (_, "Want") => vec![A::Want],
        (_, "Requested") => vec![A::Requested],
        (_, "Favourite, Rating, Download\u{2026}, Sync, Delete, Edit") => {
            vec![A::Favourite, A::Rating, A::Download, A::Sync, A::Delete { copy: 0 }, A::Edit]
        }
        (_, "Share, Web search, Go to, View") => vec![A::Share, A::WebSearch, A::GoTo, A::View],
        (_, "Favourite, Rating") => vec![A::Favourite, A::Rating],
        (_, "Edit, Change cover, Missing tracks\u{2026}") => vec![A::Edit, A::ChangeCover, A::MissingTracks],
        (_, "Save to, Move to MORE, Replace in MUSIC") => vec![
            A::SaveTo { copy: 0, to: own_lib(OWN_B, "bob") },
            A::MoveTo { copy: 0, to: main_lib(MORE) },
            A::ReplaceIn { into: main_lib(MUSIC) },
        ],
        (_, "Move to IN, Move to IN2 (F34)") => vec![
            A::MoveTo { copy: 0, to: incoming_lib(IN, Fetcher::Lidarr, None) },
            A::MoveTo { copy: 0, to: incoming_lib(IN2, Fetcher::Soulseek, None) },
        ],
        (_, "Delete album") => vec![A::Delete { copy: 0 }],
        (_, "Download\u{2026}, Sync to this device") => vec![A::Download, A::Sync],
        (_, "Edit") => vec![A::Edit],
        (_, "Save to MUSIC") => vec![A::SaveTo { copy: 0, to: main_lib(MUSIC) }],
        (_, "Move, Replace, Delete of that copy") => {
            vec![A::MoveTo { copy: 0, to: main_lib(MORE) }, A::ReplaceIn { into: main_lib(MUSIC) }, A::Delete { copy: 0 }]
        }
        (_, "Cancel conversion") => vec![A::CancelConversion { copy: 0 }],
        (_, "Add to Your music (OWN-A, her default)") => vec![A::AddTo { copy: 1, to: own_lib(OWN_A, "alice") }],
        ("arrival in IN, asked by alice", "Add to MUSIC") => vec![A::AddTo { copy: 1, to: main_lib(MUSIC) }],
        ("arrival in IN, asked by alice", "Replace in MUSIC (better)") => vec![A::Replace { copy: 1, into: main_lib(MUSIC) }],
        ("arrival in IN, asked by alice", "Delete") => vec![A::DeleteArrival { copy: 1 }],
        (_, "Undo, after root's Add to MUSIC") => vec![A::Undo { copy: 1 }],
        (_, "Add to MUSIC") => vec![A::AddTo { copy: 0, to: main_lib(MUSIC) }],
        (_, "Delete") => vec![A::DeleteArrival { copy: 0 }],
        (_, "Add to (his default)") => vec![A::AddTo { copy: 0, to: own_lib(OWN_B, "bob") }],
        (_, "Replace in MUSIC (UP03.4)") => vec![A::Replace { copy: 0, into: main_lib(MUSIC) }],
        _ => panic!("an action the table names that this test does not know: {state} / {action}"),
    }
}

/// The table's words for an answer: "yes" Allow; "ask" AskFirst; "no: right" a 403; "no: state" a 409; "no" either;
/// "none" (and "none: hidden") not shown at all, a 404.
fn holds(answer: &str, d: Decision) -> bool {
    match answer {
        "yes" => d == Decision::Allow,
        "ask" => matches!(d, Decision::AskFirst(_)),
        "no: right" => matches!(d, Decision::Deny(Why::Right(_))),
        "no: state" => matches!(d, Decision::Deny(Why::State(_))),
        "no" => matches!(d, Decision::Deny(Why::Right(_) | Why::State(_))),
        "none" | "none: hidden" => d == Decision::Deny(Why::Hidden),
        other => panic!("an answer the table does not know: {other}"),
    }
}

/// CN1. Red first: no `decide` today; the rights are worked out in the page (`offersOf`, `mayEdit`, `copyPlaying`) and in
/// each handler apart (app/albums.rs:30-48, serve/copies.rs:185, api.rs:287-289).
#[test]
fn the_policy_table_is_the_spec() {
    let table: Table = serde_json::from_str(POLICY).unwrap();
    assert!(table.about.contains("CN1"));
    assert_eq!(table.viewers, ["root", "key", "control", "admin2", "alice", "bob", "carol"]);
    let states: BTreeSet<&str> = table.rows.iter().map(|r| r.state.as_str()).collect();
    assert_eq!((states.len(), table.rows.len()), (8, 29), "every state and row of CN1's table, the Play row split by ruling 1");
    for row in &table.rows {
        assert_eq!(row.answers.len(), table.viewers.len(), "{row:?}");
        for (name, answer) in table.viewers.iter().zip(&row.answers) {
            // "—": no row for that viewer.
            if answer == "\u{2014}" {
                continue;
            }
            let v = viewer(name);
            let record = if row.action.starts_with("Undo") { kept_by_root(&v) } else { fixture(&row.state, &v) };
            for action in actions(&row.state, &row.action) {
                let d = decide(&v, &action, &record);
                assert!(holds(answer, d), "{} / {action:?} for {name}: the table says {answer}, decide gives {d:?}", row.state);
                // Never a reason without words.
                if let Decision::Deny(why) = d {
                    assert!(!why.words().is_empty(), "{why:?}");
                    assert!([403, 404, 409].contains(&why.status()));
                }
            }
        }
    }
}

/// TY4: the policy table as shared JSON, in its written form, for the page's tests to read (shared/policy.json; shared/words.json is
/// 0097's). Red first: the table sat beside the crate (crates/domain/tests/policy.json), where no page test reads it.
#[test]
fn the_tables_are_written_and_current() {
    let table: Table = serde_json::from_str(POLICY).unwrap();
    let written = serde_json::to_string_pretty(&table).unwrap() + "\n";
    assert_eq!(POLICY.replace('\r', ""), written, "shared/policy.json is not in its written form");
    let beside = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/policy.json");
    assert!(!beside.exists(), "one copy of a table: none beside the crate");
}

/// CN1's Never: an action on an upload offered to anyone the upload is hidden from.
#[test]
fn no_action_on_an_upload_reaches_whom_it_is_hidden_from() {
    for name in ["alice", "carol"] {
        let v = viewer(name);
        let upload = fixture("bob's upload in UPL, waiting", &v);
        let on_it = [
            A::AddTo { copy: 0, to: own_lib(OWN_A, "alice") },
            A::AddTo { copy: 0, to: main_lib(MUSIC) },
            A::Replace { copy: 0, into: main_lib(MUSIC) },
            A::DeleteArrival { copy: 0 },
            A::Delete { copy: 0 },
            A::PlayFrom { copy: 0 },
            A::SaveTo { copy: 0, to: main_lib(MUSIC) },
            A::MoveTo { copy: 0, to: main_lib(MORE) },
            A::CancelConversion { copy: 0 },
            A::Undo { copy: 0 },
        ];
        for action in on_it {
            assert_eq!(decide(&v, &action, &upload), Decision::Deny(Why::Hidden), "{action:?} for {name}");
            assert!(!can(&v, &action, &upload));
        }
        assert_eq!(Why::Hidden.status(), 404);
        // Her holding of it: not had, as if it were not there (HD6).
        assert_eq!(upload.holding, Holding::NotHad);
    }
}

/// A table the server's code holds, kept in shared/ in its written form for the page's tests (TY4): written where it differs, and
/// then the test fails until the written file is committed, so a table changed in code and not written again fails.
fn written(file: &str, table: &impl Serialize) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../shared").join(file);
    let json = serde_json::to_string_pretty(table).unwrap() + "\n";
    let kept = std::fs::read_to_string(&path).unwrap_or_default().replace('\r', "");
    if kept != json {
        std::fs::write(&path, &json).unwrap();
        panic!("shared/{file} was not the table's written form: written now, commit it");
    }
}

/// RS1: the when-table is data, read whole. The sections' one order, and a row a place: its context, the section right after the
/// head, its library set, whether View is offered. shared/when.json, for the page's check that each opener sends its `from`.
/// Red first: no when-table on the server (each sheet's own order, PlaylistView's and AlbumMenu's).
#[test]
fn the_when_table_is_read_whole() {
    use cratarr_domain::when::{Context, Libraries, ORDER, Section, WHEN};
    #[derive(Serialize)]
    struct Table {
        about: &'static str,
        order: Vec<Section>,
        places: Vec<cratarr_domain::when::When>,
    }
    let about = "The record sheet's when-table (SCENARIOS-DC1 RS1; decision 0104 point 6), written by cratarr-domain's tests: the sections' one order, and per place (its `from`) only what it adds: its context, the section right after the head, the libraries it counts, and whether View is offered.";
    assert_eq!(ORDER[0], Section::Head);
    assert_eq!(ORDER.len(), 13);
    let row = |from: &str| WHEN.iter().find(|w| w.from == from).unwrap();
    assert_eq!((row("crate").first, row("crate").context, row("crate").libraries), (Section::Context, Some(Context::Crate), Libraries::Crate));
    assert_eq!((row("incoming").first, row("problem").first, row("downloads").first), (Section::Arrival, Section::Health, Section::Device));
    assert_eq!(row("library").libraries, Libraries::Switch);
    assert!(!row("page").view);
    written("when.json", &Table { about, order: ORDER.to_vec(), places: WHEN.to_vec() });
}

/// TY4 for the record's words (RS2, RS23): the states' words and the entries, as shared JSON in their written form.
#[test]
fn the_words_tables_are_written_and_current() {
    use cratarr_domain::words::{ENTRIES, Entry, STATES, StateWords};
    #[derive(Serialize)]
    struct States {
        about: &'static str,
        states: Vec<StateWords>,
    }
    #[derive(Serialize)]
    struct Entries {
        about: &'static str,
        entries: Vec<Entry>,
    }
    written(
        "record-states.json",
        &States {
            about: "What each state of a record says (SCENARIOS-DC1 RS2), written by cratarr-domain's tests: its words with their blanks; a time beside them is the page's to say.",
            states: STATES.to_vec(),
        },
    );
    written(
        "entries.json",
        &Entries {
            about: "The record sheet's entries (SCENARIOS-DC1 RS23), written by cratarr-domain's tests: each one's name in the answers, its title with its blank ({type}, {name}, {library}), its icon and its group, in the menu's order.",
            entries: ENTRIES.to_vec(),
        },
    );
}
