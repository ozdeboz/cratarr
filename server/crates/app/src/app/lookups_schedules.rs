//! SCENARIOS-LOOKUPS LK.S1: every schedule a moved lookup hangs on is started at the server's start, on the runner. It begins every
//! loop the server's start begins, so it runs in a process of its own, by name (`--ignored lookups_schedules`), as LK.0's scan does.

use std::path::Path;
use std::sync::Arc;

use crate::app::App;
use crate::serve::Serve;

/// A server on `data` as `main` opens one, its settings file read; the process's server and journal set, as the boot sets them.
async fn open(data: &Path) -> (Arc<App>, Arc<Serve>) {
    let dir = data.to_string_lossy().into_owned();
    let env = |k: &str| (k == "CRATARR_DATA_DIR").then(|| dir.clone());
    let cfg = crate::config::Config::from_lookup(env).unwrap();
    let saved = crate::settings::load(&data.join("settings.json"), &env).unwrap().expect("its settings");
    let served = Serve::open(Some(data), saved.tag_rules.clone(), &env).unwrap();
    crate::serve::set_instance(served.clone());
    let app = App::new(cfg, saved, crate::settings::Origin::File, Some(served.clone())).await.unwrap();
    crate::journal::set_instance(app.journal.clone());
    served.attach_catalogue(app.library.catalogue());
    served.library.set_kinds(app.library.folder_kinds());
    (app, served)
}

/// The start function's own text, between its first line and its end.
fn loops_text() -> &'static str {
    let text = include_str!("start.rs");
    let from = text.find("pub fn loops(").expect("the start function");
    let to = text[from..].find("\n}\n").map_or(text.len(), |n| from + n);
    &text[from..to]
}

/// LK.S1 (R-N1/N2, R-M5, T-LK-5): the server's start (`start::loops`) begins each schedule a moved lookup hangs on. The live lines
/// are in `schedules()` once it ends, each named by its job kind with its period: the records' due read, Soulseek's round, Spotify's
/// look; paused with background reading where their tick reads the outside world. Each waiting line (a loop still bare, moving in
/// its own run) is begun by the start function: its call is in it. The arrival's job, which has no period, is begun by the record
/// pass when it finds an arrival in an Incoming library. A second schedule of a kind is refused. Never a moved lookup whose only
/// start is a GET or nothing; never a second instance of a schedule.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "LK.S1: begins every loop the server's start begins, in a process of its own (run by name)"]
async fn every_schedule_a_moved_lookup_hangs_on_is_started() {
    use crate::app::reach::stand_ins::{Asked, StandIns};
    let _state = crate::app::references::TestState::set().await;
    // Every outside ask answered here, none sent: nothing found.
    let stand_ins = StandIns::set(Arc::new(|_: &Asked| Some((404, String::new())))).await;
    let root = std::env::temp_dir().join(format!("cratarr-lk-s1-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (music, incoming, data) = (root.join("music"), root.join("incoming"), root.join("data"));
    for d in [&music, &incoming, &data] {
        std::fs::create_dir_all(d).unwrap();
    }
    let (m, i) = (music.to_string_lossy().into_owned(), incoming.to_string_lossy().into_owned());
    let kind = crate::source::cratarr::KIND.id;
    let settings = serde_json::json!({
        "version": 4,
        "music_folder": m,
        "sources": [{ "id": kind, "kind": kind, "enabled": true,
            "libraries": [{ "id": m, "kind": "main" }, { "id": i, "kind": "incoming" }], "values": { "folders": [m, i] } }],
    });
    std::fs::write(data.join("settings.json"), serde_json::to_vec_pretty(&settings).unwrap()).unwrap();
    let (app, served) = open(&data).await;
    // The boot begins no schedule (LK.0's harness 3): only the start does.
    let before: Vec<String> = crate::runner::runner().schedules().into_iter().map(|s| s.kind).collect();
    crate::app::start::loops(&app);
    let schedules = crate::runner::runner().schedules();
    let live = [
        (crate::app::references::READ_KIND, crate::app::references::DUE_EVERY.as_secs()),
        (crate::app::soulseek::ROUND, crate::app::soulseek::ROUND_EVERY.as_secs()),
        (crate::app::spotify::LOOK_KIND, crate::app::spotify::LOOK.as_secs()),
    ];
    for (kind, period) in live {
        assert!(!before.contains(&kind.to_string()), "{kind}: begun by the boot, not the start");
        let s = schedules.iter().find(|s| s.kind == kind).unwrap_or_else(|| panic!("{kind} not begun by the start: {schedules:?}"));
        assert_eq!(s.period_secs, period, "{kind}'s period");
    }
    // The waiting lines: each loop still bare is begun by the start function (its call in it).
    let text = loops_text();
    for call in crate::app::start::BEGINS {
        assert!(text.contains(call), "{call} is not begun by the server's start");
    }
    // One instance of a schedule.
    let again = crate::runner::runner().every(crate::app::spotify::LOOK_KIND, std::time::Duration::from_secs(1), || false, || async { None::<(String, ())> }, |_| async { anyhow::Ok(()) });
    assert!(again.is_err(), "a second schedule of a kind is refused");
    // Paused with background reading, as Activity says it (AC4): the reads of the outside world.
    let was = crate::app::switches::set_for_tests(crate::app::switches::BACKGROUND, false);
    let paused: Vec<(String, bool)> = crate::runner::runner().schedules().into_iter().map(|s| (s.kind, s.paused)).collect();
    crate::app::switches::set_for_tests(crate::app::switches::BACKGROUND, was);
    for kind in [crate::app::references::READ_KIND, crate::app::spotify::LOOK_KIND] {
        assert!(paused.iter().any(|(k, p)| k == kind && *p), "{kind} paused with background reading: {paused:?}");
    }
    // The arrival's job: the record pass finds an arrival in Incoming, and its job begins.
    let dir = incoming.join("Kite Theory").join("Untitled Shore");
    std::fs::create_dir_all(&dir).unwrap();
    for n in 1..=3 {
        std::fs::write(dir.join(format!("{n:02} Shore {n}.flac")), format!("track {n}")).unwrap();
    }
    let cat = app.library.catalogue();
    let folder = cat.folders().id_of(&incoming).expect("the Incoming folder in use");
    cat.walk_all();
    cat.pretend_read_for_tests();
    for n in 1..=3 {
        let (title, number) = (format!("Shore {n}"), n.to_string());
        cat.tag_for_tests(folder, &format!("Kite Theory/Untitled Shore/{n:02} Shore {n}.flac"), &[("album", "Untitled Shore"), ("albumartist", "Kite Theory"), ("artist", "Kite Theory"), ("title", title.as_str()), ("tracknumber", number.as_str())]);
    }
    cat.identify().unwrap();
    let s = served.clone();
    crate::run::spawn_blocking(move || s.refresh()).await.unwrap().unwrap();
    let record = served.library.album_in(folder, "Kite Theory/Untitled Shore").unwrap().expect("the arrival built");
    let mut begun = false;
    for _ in 0..1000 {
        if crate::runner::runner().follow(crate::app::arrival_job::KIND, &record, "").is_some() {
            begun = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(begun, "the record pass's arrival begins its job");
    drop(stand_ins);
    let _ = std::fs::remove_dir_all(&root);
}
