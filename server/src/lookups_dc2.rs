//! SCENARIOS-LOOKUPS' DC2 families through the whole router (TS item 5's in-process harness): a GET reads only what's stored and
//! asks no outside source; every outside lookup is a runner job begun by an open (a POST) or a schedule, and an open answers at
//! once from what's kept (R-M2). The stand-ins count what each source is asked.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use crate::whole_router::Whole;

/// Night Ferry's release and the next album's, as their files name them.
const R1: &str = "a1b2c3d4-0000-4000-8000-000000000042";
const R2: &str = "a1b2c3d4-0000-4000-8000-000000000043";

/// `path` asked by that cookie with `body` (none: no body): the answer's status and JSON.
async fn ask(w: &Whole, cookie: &str, method: Method, path: &str, body: Value) -> (StatusCode, Value) {
    let body = if body.is_null() { Body::empty() } else { Body::from(body.to_string()) };
    w.send(("cookie", cookie), method, path, body).await
}

/// Night Ferry (8 of its release's 10 tracks) and Signal (3), each of its files naming its MusicBrainz release, in the server's
/// Music: their album ids.
fn plant(w: &Whole) -> (String, String) {
    let cat = w.app.library.catalogue();
    let mut files = Vec::new();
    for (album, release, n) in [("Night Ferry", R1, 8u32), ("Signal", R2, 3)] {
        for t in 1..=n {
            let rel = format!("Glass Harbour/{album}/{t:02} Track {t}.flac");
            let path = w.dir().join(&rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &rel).unwrap();
            files.push((rel, album, release, t));
        }
    }
    cat.walk_all();
    cat.pretend_read_for_tests();
    for (rel, album, release, t) in &files {
        let (title, number) = (format!("Track {t}"), t.to_string());
        let tags = [("album", *album), ("albumartist", "Glass Harbour"), ("artist", "Glass Harbour"), ("title", title.as_str()), ("tracknumber", number.as_str()), ("musicbrainz_albumid", *release)];
        cat.tag_for_tests(0, rel, &tags);
    }
    cat.identify().unwrap();
    w.served.refresh().unwrap();
    let id = |dir: &str| w.served.library.album_in(0, dir).unwrap().unwrap_or_else(|| panic!("{dir} built"));
    (id("Glass Harbour/Night Ferry"), id("Glass Harbour/Signal"))
}

/// A release as MusicBrainz lists it: `n` tracks, "Track 1" …, each 200 s.
fn release_json(id: &str, title: &str, n: u32) -> Value {
    let tracks: Vec<Value> = (1..=n)
        .map(|t| json!({ "id": format!("t-{t}"), "position": t, "title": format!("Track {t}"), "length": 200_000,
            "recording": { "id": format!("{id}-rec-{t}") }, "artist-credit": [{ "name": "Glass Harbour", "artist": { "id": "ar-gh" } }] }))
        .collect();
    json!({ "id": id, "title": title, "date": "2012", "artist-credit": [{ "name": "Glass Harbour", "artist": { "id": "ar-gh" } }],
        "media": [{ "position": 1, "format": "CD", "tracks": tracks }] })
}

/// LK.B2 (row 28) with N8's follow-by-key: Replace's release is read by a job its open begins, never by its GET. (a) the GET before
/// any open answers 404 in the one shape, asking nothing; (b) root's open answers 202 at once with its task while MusicBrainz is
/// held, the read-ahead of the next album another job, both run, MusicBrainz asked once for each; root's `GET /api/tasks` lists
/// both tasks (alice's none); the GET then answers the kept release, each track with the file that holds it; (c) opened again,
/// 200 with the kept release, nothing asked; (d) MusicBrainz switched off, another album's open refused in F1's words, nothing
/// asked; (e) alice's open is 403. Never MusicBrainz asked under the GET or inside the POST's handler; never the POST waiting on
/// MusicBrainz' answer; never one album's read refused because another's runs. Red first: the GET asks MusicBrainz, no POST.
#[tokio::test]
async fn a_releases_tracks_are_read_by_a_job_not_its_get() {
    let w = Whole::open("lk-b2").await;
    w.served.users.from_directory("alice", false, None).unwrap();
    let (root, alice) = (w.session("me"), w.session("alice"));
    let (night, signal) = plant(&w);
    let asked = Arc::new(AtomicUsize::new(0));
    let (hold, held) = tokio::sync::watch::channel(true);
    let stand_in: crate::app::track_fetch::ReleaseStandIn = {
        let asked = asked.clone();
        Arc::new(move |id: String| {
            asked.fetch_add(1, Ordering::SeqCst);
            let mut held = held.clone();
            Box::pin(async move {
                while *held.borrow() {
                    held.changed().await?;
                }
                Ok(if id == R1 { release_json(R1, "Night Ferry", 10) } else { release_json(R2, "Signal", 3) })
            })
        })
    };
    crate::app::track_fetch::set_release_stand_in(Some(stand_in));
    let release = |album: &str| format!("/api/albums/cratarr/{album}/release");
    // (a) Before any open: 404 in the one shape, nothing asked.
    let (status, answer) = ask(&w, &root, Method::GET, &release(&night), Value::Null).await;
    // The one table's words (cratarr_domain::words::RELEASE_NOT_READ).
    assert_eq!((status, answer["error"].as_str()), (StatusCode::NOT_FOUND, Some("That release isn't read yet")), "{answer}");
    assert_eq!(asked.load(Ordering::SeqCst), 0, "the GET asks nothing");
    // (b) Root's open and the read-ahead: 202 at once with a task each, MusicBrainz held the while.
    let (status, opened) = ask(&w, &root, Method::POST, &release(&night), Value::Null).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{opened}");
    let night_task = opened["task"]["id"].as_u64().unwrap_or_else(|| panic!("its task: {opened}"));
    let (status, ahead) = ask(&w, &root, Method::POST, &release(&signal), Value::Null).await;
    assert_eq!(status, StatusCode::ACCEPTED, "another album's read is never refused: {ahead}");
    let signal_task = ahead["task"]["id"].as_u64().unwrap();
    assert_ne!(night_task, signal_task, "one job per album");
    let (_, tasks) = ask(&w, &root, Method::GET, "/api/tasks", Value::Null).await;
    let listed: Vec<u64> = tasks["tasks"].as_array().unwrap().iter().filter_map(|t| t["id"].as_u64()).collect();
    assert!(listed.contains(&night_task) && listed.contains(&signal_task), "root follows both: {tasks}");
    let (_, theirs) = ask(&w, &alice, Method::GET, "/api/tasks", Value::Null).await;
    assert!(theirs["tasks"].as_array().unwrap().iter().all(|t| t["id"].as_u64() != Some(night_task)), "alice follows nothing: {theirs}");
    hold.send(false).unwrap();
    let ended = |id: u64| {
        let w = &w;
        let root = &root;
        async move {
            for _ in 0..200 {
                let (_, tasks) = ask(w, root, Method::GET, "/api/tasks", Value::Null).await;
                if let Some(t) = tasks["tasks"].as_array().unwrap().iter().find(|t| t["id"].as_u64() == Some(id) && !t["finished"].is_null()) {
                    return t.clone();
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            panic!("task {id} never ended");
        }
    };
    let done = ended(night_task).await;
    assert!(done["error"].is_null(), "a clean end: {done}");
    ended(signal_task).await;
    assert_eq!(asked.load(Ordering::SeqCst), 2, "MusicBrainz asked once for each album");
    let (status, view) = ask(&w, &root, Method::GET, &release(&night), Value::Null).await;
    assert_eq!(status, StatusCode::OK, "the GET answers the kept release: {view}");
    let tracks = view["tracks"].as_array().unwrap();
    assert_eq!(tracks.len(), 10);
    assert_eq!(tracks.iter().filter(|t| !t["have"].is_null()).count(), 8, "each track with the file that holds it: {view}");
    // (c) Opened again: 200 with the kept release, nothing asked.
    let (status, again) = ask(&w, &root, Method::POST, &release(&night), Value::Null).await;
    assert_eq!((status, again["release"].as_str()), (StatusCode::OK, Some(R1)), "{again}");
    assert_eq!(asked.load(Ordering::SeqCst), 2, "nothing asked for a kept release");
    // (e) An administrator's action.
    let (status, _) = ask(&w, &alice, Method::POST, &release(&night), Value::Null).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // (d) MusicBrainz switched off: another album's open refused in F1's words, nothing asked.
    let asked_off = Arc::new(AtomicUsize::new(0));
    let counting = asked_off.clone();
    crate::app::track_fetch::set_release_stand_in(Some(Arc::new(move |_: String| {
        counting.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { anyhow::bail!("asked") })
    })));
    let (status, off) = {
        let _state = crate::app::switches::TEST_STATE.write().await;
        let was = crate::app::switches::set_for_tests(crate::app::switches::MUSICBRAINZ, false);
        let answer = ask(&w, &root, Method::POST, &release(&signal), Value::Null).await;
        crate::app::switches::set_for_tests(crate::app::switches::MUSICBRAINZ, was);
        answer
    };
    assert_eq!((status, off["error"].as_str()), (StatusCode::CONFLICT, Some("MusicBrainz is switched off")), "{off}");
    assert_eq!(asked_off.load(Ordering::SeqCst), 0, "nothing asked while it is off");
    crate::app::track_fetch::set_release_stand_in(None);
}

/// A Bandcamp album page as Bandcamp serves it (its album data in `data-tralbum`), read by the client's own parser.
fn bandcamp_page(artist: &str, title: &str) -> String {
    let data = json!({ "artist": artist, "current": { "title": title }, "trackinfo": [{ "track_num": 1, "title": "Slack Water", "duration": 200.0 }] }).to_string();
    format!(r#"<html><body><script data-tralbum="{}"></script></body></html>"#, data.replace('"', "&quot;"))
}

/// LK.P3 (P3, P4; SCENARIOS-DC3 ENT.16): a Want names its record by what is kept or sent; its page is read by the record's job, never
/// in the Want's handler; a page Discover's search found by a name is never its reference. (a) Root Wants "Tide Tables" by its page
/// and names while Bandcamp is held: answered at once with the request and the record's read's task, the record made by its page (a
/// link); once Bandcamp answers, the job reads the page once. (b) Wanted again: the record found, nothing asked. (d) "Lamplight" by
/// its release group with the page Discover's search found by name: the record made by the group, no Bandcamp reference kept, and its
/// job reads no Bandcamp page. Never a Bandcamp page read in a Want's handler, nor a Want waiting on Bandcamp; never a page Discover's
/// search found kept as a record's reference or read for it. Red first: the Want read the page in its handler.
#[tokio::test]
async fn a_want_reads_no_page_in_its_handler() {
    use crate::app::reach::stand_ins::{Asked, StandIns};
    // The switches on and Bandcamp held back by nothing, as the process shares them (one test at a time that changes them).
    let _state = crate::app::references::TestState::set().await;
    let w = Whole::open("lk-p3").await;
    let root = w.session("me");
    const TIDE: &str = "https://glassharbour.bandcamp.com/album/tide-tables";
    const LAMP: &str = "https://kitetheory.bandcamp.com/album/lamplight";
    const G: &str = "00000000-0000-4000-8000-000000000377";
    let stand_ins = StandIns::set(Arc::new(|a: &Asked| match (a.source, a.path.as_str()) {
        ("bandcamp", "/album/tide-tables") => Some((200, bandcamp_page("Glass Harbour", "Tide Tables"))),
        ("bandcamp", "/album/lamplight") => Some((200, bandcamp_page("Kite Theory", "Lamplight"))),
        // Anything else: no such page (never a server's error, which would hold a source back for every test in the process).
        _ => Some((404, String::new())),
    }))
    .await;
    let page_reads = |path: &str| stand_ins.asked().iter().filter(|a| a.source == "bandcamp" && a.path == path).count();
    stand_ins.hold("bandcamp");
    // (a) Answered at once, Bandcamp held the while: the handler waits on no page.
    let want = json!({ "page": TIDE, "artist": "Glass Harbour", "album": "Tide Tables" });
    let (status, wanted) = tokio::time::timeout(std::time::Duration::from_secs(10), ask(&w, &root, Method::POST, "/api/requests/want", want.clone())).await.expect("never waiting on Bandcamp");
    assert_eq!(status, StatusCode::OK, "{wanted}");
    let cat = w.served.catalogue().clone();
    let record = cat.record_named_by(None, Some(TIDE)).unwrap().expect("the record made by its page");
    let refs = cat.references_of(&crate::serve::ids::record(record)).unwrap();
    let rows: Vec<String> = crate::runner::runner().rows().iter().map(|r| format!("{} {:?}", r.kind, r.state)).collect();
    assert!(wanted["task"]["id"].as_u64().is_some(), "the record's read begun, its task beside the request: {wanted}; refs {refs:?}; jobs {rows:?}; had {:?}", cat.record(record).unwrap().map(|r| r.had));
    assert!(refs.iter().any(|(r, row)| r.source == "bandcamp" && r.value == TIDE && row.how != "search"), "its page a link: {refs:?}");
    stand_ins.let_go();
    for _ in 0..400 {
        if page_reads("/album/tide-tables") > 0 && crate::runner::runner().rows().iter().all(|r| r.kind != crate::app::references::READ_KIND) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(page_reads("/album/tide-tables"), 1, "the record's job read the page once, by its link");
    // (b) Wanted again: the request found, nothing more asked.
    let (status, again) = ask(&w, &root, Method::POST, "/api/requests/want", want).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(page_reads("/album/tide-tables"), 1, "within its day: never read twice");
    // (d) A page Discover's search found by name: a guess, never the record's reference nor read for it.
    w.served.userdata.set_bandcamp_found(G, crate::app::moment::now_secs(), Some(LAMP)).unwrap();
    let (status, lamp) = ask(&w, &root, Method::POST, "/api/requests/want", json!({ "release_group": G, "page": LAMP, "artist": "Kite Theory", "album": "Lamplight" })).await;
    assert_eq!(status, StatusCode::OK, "{lamp}");
    let record = cat.record_named_by(Some(G), None).unwrap().expect("the record made by its release group");
    let refs = cat.references_of(&crate::serve::ids::record(record)).unwrap();
    assert!(refs.iter().all(|(r, _)| r.source != "bandcamp"), "no Bandcamp reference from Discover's search: {refs:?}");
    for _ in 0..200 {
        if crate::runner::runner().rows().iter().all(|r| r.kind != crate::app::references::READ_KIND) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(page_reads("/album/lamplight"), 0, "the searched page never read for the record");
}
