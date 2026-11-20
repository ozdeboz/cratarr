//! The whole router, swept (the audit, 2026-10-29: one harness over all the routes). Every route the source names
//! (`.route("…", …)` outside tests) is asked for twice through the router `main` serves: with no way in it is refused
//! before any handler runs (only the open routes pass), and signed in as an administrator its path is there (a method
//! it has no handler for answers 405, not the fallback's). No handler runs, so nothing is asked of a source or the
//! network.

use std::path::Path;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use tower::ServiceExt;

/// Each route the source names: its path (each `{param}` a plain value) and its methods.
fn named_routes() -> Vec<(String, Vec<Method>)> {
    let mut files = Vec::new();
    let mut dirs = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), Path::new(env!("CARGO_MANIFEST_DIR")).join("crates")];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                dirs.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") && p.file_name().is_some_and(|n| n != "whole_router.rs") {
                files.push(p);
            }
        }
    }
    let mut out = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        // The tests' own routers are not the server's, nor the routers of the helpers shared with the tests of the crates
        // above (`test-support`, SPLIT-PLAN 5.4), which stand where test modules do.
        let cut = ["#[cfg(test)]", "#[cfg(any(test, feature = \"test-support\"))]"].iter().filter_map(|m| text.find(m)).min().unwrap_or(text.len());
        let text = &text[..cut];
        let mut rest = text;
        while let Some(at) = rest.find(".route(\"") {
            rest = &rest[at + 8..];
            let Some(end) = rest.find('"') else { break };
            let path = rest[..end].to_string();
            if !path.starts_with('/') {
                continue;
            }
            let call = &rest[end..rest[end..].find(".route(\"").map_or(rest.len(), |n| end + n)];
            let methods: Vec<Method> = [("get(", Method::GET), ("post(", Method::POST), ("put(", Method::PUT), ("delete(", Method::DELETE), ("patch(", Method::PATCH)]
                .into_iter()
                .filter(|(name, _)| call.match_indices(name).any(|(i, _)| i == 0 || !call.as_bytes()[i - 1].is_ascii_alphanumeric() && call.as_bytes()[i - 1] != b'_'))
                .map(|(_, m)| m)
                .collect();
            let plain = path.split('/').map(|part| if part.starts_with('{') { "x" } else { part }).collect::<Vec<_>>().join("/");
            out.push((plain, methods));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// The server as `main` serves it, signing in required, with one account, the administrator `me`, and a folder of its own (Home
/// Assistant's contract, `home_assistant`, asks it too).
pub(crate) struct Whole {
    dir: std::path::PathBuf,
    pub(crate) served: Arc<crate::serve::Serve>,
    pub(crate) app: Arc<crate::app::App>,
    pub(crate) router: axum::Router,
}

impl Whole {
    pub(crate) async fn open(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("cratarr-whole-router-{name}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(crate::playlists::PLAYLIST_DIR)).unwrap();
        let served = crate::serve::Serve::open(None, crate::catalogue::Rules::default(), &|_| None).unwrap();
        served.users.set("me", "pw").unwrap();
        served.sessions.set_required(true).unwrap();
        let cfg = crate::config::Config::from_lookup(|_| None).unwrap();
        let saved = crate::settings::Saved { music_folder: Some(dir.to_string_lossy().into_owned()), ..Default::default() };
        let app = crate::app::App::new(cfg, saved, crate::settings::Origin::Env, Some(Arc::clone(&served))).await.unwrap();
        // As `main` does: the server answers from the app's catalogue (requests, the record pages).
        served.attach_catalogue(app.library.catalogue());
        let router = super::routes(app.clone(), Arc::clone(&served));
        Self { dir, served, app, router }
    }

    /// A sign-in of `user`, as the cookie a browser sends.
    fn session(&self, user: &str) -> String {
        let (token, _) = self.served.sessions.create(user, "the sweep", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64).unwrap();
        format!("{}={token}", crate::serve::signin::COOKIE)
    }

    /// An API key of `user`'s, as the `authorization` header a client sends.
    pub(crate) fn key(&self, user: &str, scope: &str) -> String {
        let (key, _) = self.served.sessions.new_key("a client", Some(user), scope, None, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64).unwrap();
        format!("Bearer {key}")
    }

    /// `path` asked with that cookie: the answer's status, and its body as JSON (null where it is none).
    async fn ask(&self, cookie: &str, method: Method, path: &str) -> (StatusCode, serde_json::Value) {
        self.send(("cookie", cookie), method, path, Body::empty()).await
    }

    /// `path` asked that way, with `body`: the answer as `ask` gives it.
    pub(crate) async fn send(&self, by: (&str, &str), method: Method, path: &str, body: Body) -> (StatusCode, serde_json::Value) {
        let req = Request::builder().method(method).uri(path).header(by.0, by.1).header("accept", "application/json").header("content-type", "application/json").body(body).unwrap();
        let r = self.router.clone().oneshot(req).await.unwrap();
        let status = r.status();
        let body = axum::body::to_bytes(r.into_body(), 1 << 24).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null))
    }
}

impl Drop for Whole {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// `body` put as JSON to `path` with that cookie: the answer as `Whole::send` gives it.
async fn put(w: &Whole, cookie: &str, path: &str, body: serde_json::Value) -> (StatusCode, serde_json::Value) {
    w.send(("cookie", cookie), Method::PUT, path, Body::from(body.to_string())).await
}

/// AL04.4 (the review of run 13, RM8): a room and the floor are a number of GB, from 0. A non-number (`"x"`, `null`, a number as text,
/// none at all) or a negative is refused with 400 and the words the Users panel and Settings show, never axum's 422, and the old value
/// is kept.
#[tokio::test]
async fn a_room_or_floor_must_be_a_number_of_gb() {
    let w = Whole::open("gb").await;
    let me = w.session("me");
    w.served.users.create("ann", "ann pw", false).unwrap();
    assert!(put(&w, &me, "/api/users/ann/room", serde_json::json!({ "gb": 50 })).await.0.is_success());
    assert!(put(&w, &me, "/api/settings/floor", serde_json::json!({ "gb": 300 })).await.0.is_success());
    for bad in [serde_json::json!({ "gb": "x" }), serde_json::json!({ "gb": null }), serde_json::json!({ "gb": "50" }), serde_json::json!({}), serde_json::json!({ "gb": -1 })] {
        let (status, answer) = put(&w, &me, "/api/users/ann/room", bad.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "room {bad}: {answer}");
        assert_eq!(answer["error"], "Give the room in GB, from 0.", "room {bad}: {answer}");
        let (status, answer) = put(&w, &me, "/api/settings/floor", bad.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "floor {bad}: {answer}");
        assert!(answer["error"].as_str().is_some_and(|e| e.starts_with("Give the floor in GB, from 0")), "floor {bad}: {answer}");
    }
    assert_eq!(w.served.users.get("ann").unwrap().room_bytes, Some(50 * crate::app::room::GB), "the old room kept");
    assert_eq!(crate::app::room::floor(&w.served.userdata), 300 * crate::app::room::GB, "the old floor kept");
}

/// `body` posted as JSON to `path` with that cookie: the answer as `Whole::send` gives it.
async fn post(w: &Whole, cookie: &str, path: &str, body: serde_json::Value) -> (StatusCode, serde_json::Value) {
    w.send(("cookie", cookie), Method::POST, path, Body::from(body.to_string())).await
}

/// W1: a Want over HTTP takes the record it names, by each way a card can name one: its id as the lists serve it (any other kind
/// of id refused), its release group, its Bandcamp page as written (made plain; another site's page refused), its names (a record
/// not had made), and the names of an album the library has (that record).
#[tokio::test]
async fn every_kind_of_want_takes_its_record_over_http() {
    let w = Whole::open("wants").await;
    let me = w.session("me");
    let cat = w.served.catalogue().clone();
    let record_of = |answer: &serde_json::Value| answer["request"]["record"].as_i64();
    let set = |record: i64, source: &str, kind: &str, value: &str| assert!(cat.set_reference(&crate::serve::ids::record(record), &crate::catalogue::records::Ref::new(source, kind, value), "link").unwrap());
    let shown = cat.record_for_tests("Tide Tables (Deluxe)", 0);
    let (status, answer) = post(&w, &me, "/api/requests/want", serde_json::json!({ "record": crate::serve::ids::record(shown) })).await;
    assert_eq!((status, record_of(&answer)), (StatusCode::OK, Some(shown)), "by its id: {answer}");
    for other in [serde_json::json!({ "record": "ar-1" }), serde_json::json!({ "record": shown })] {
        assert!(!post(&w, &me, "/api/requests/want", other.clone()).await.0.is_success(), "{other}");
    }
    let grouped = cat.record_for_tests("Two Rooms", 0);
    let group = "00000000-0000-4000-8000-000000000301";
    set(grouped, "musicbrainz", "release-group", group);
    let (_, answer) = post(&w, &me, "/api/requests/want", serde_json::json!({ "release_group": group, "artist": "Glass Harbour", "album": "Two Rooms, Live" })).await;
    assert_eq!(record_of(&answer), Some(grouped), "by its group: {answer}");
    let paged = cat.record_for_tests("Night Walks (Live)", 0);
    set(paged, "bandcamp", "album", "https://kitetheory.bandcamp.com/album/night-walks");
    let (_, answer) = post(&w, &me, "/api/requests/want", serde_json::json!({ "artist": "Kite Theory", "album": "Night Walks", "page": "http://kitetheory.bandcamp.com/album/night-walks?from=search" })).await;
    assert_eq!(record_of(&answer), Some(paged), "by its page, made plain: {answer}");
    let elsewhere = serde_json::json!({ "artist": "Kite Theory", "album": "Night Walks", "page": "https://example.org/album/night-walks" });
    assert!(!post(&w, &me, "/api/requests/want", elsewhere).await.0.is_success(), "not Bandcamp's");
    let (_, answer) = post(&w, &me, "/api/requests/want", serde_json::json!({ "artist": "Orbit Lane", "album": "Quiet Lanes" })).await;
    let made = record_of(&answer).unwrap();
    assert!(![shown, grouped, paged].contains(&made) && cat.record(made).unwrap().unwrap().title == "Quiet Lanes", "by its names, made: {answer}");
    let owned = cat.record_for_tests("Harbour Lights", 2);
    let (_, answer) = post(&w, &me, "/api/requests/want", serde_json::json!({ "artist": "Glass Harbour", "album": "Harbour Lights" })).await;
    assert_eq!(record_of(&answer), Some(owned), "the library's own: {answer}");
}

/// W2 (B1): a Want over HTTP by the names of an album whose title differs from an owned album's only in brackets is a record of
/// its own: never the owned album, which the had rule would count as the remixes.
#[tokio::test]
async fn a_want_by_names_beside_an_owned_look_alike_is_not_it_over_http() {
    let w = Whole::open("look-alike").await;
    let me = w.session("me");
    let owned = w.served.catalogue().record_for_tests("Further", 2);
    let (status, answer) = post(&w, &me, "/api/requests/want", serde_json::json!({ "artist": "Glass Harbour", "album": "Further (Remixes)" })).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_ne!(answer["request"]["record"].as_i64(), Some(owned), "{answer}");
}

/// W6 (H): a Want with no title is refused over HTTP: it would make a record named nothing for life.
#[tokio::test]
async fn a_want_with_no_title_is_refused() {
    let w = Whole::open("no-title").await;
    let me = w.session("me");
    let (status, answer) = post(&w, &me, "/api/requests/want", serde_json::json!({ "artist": "Glass Harbour", "album": "  " })).await;
    assert!(!status.is_success(), "{answer}");
}

/// W1: the Bandcamp album view says what a Want of it sends: the record that holds its page.
#[tokio::test]
async fn the_bandcamp_album_view_names_the_record_that_holds_its_page() {
    let w = Whole::open("bandcamp-record").await;
    let me = w.session("me");
    let page = "https://kitetheory.bandcamp.com/album/night-walks-w1";
    crate::bandcamp::Bandcamp::get().keep(crate::bandcamp::Album {
        url: page.into(),
        title: "Night Walks".into(),
        artist: "Kite Theory".into(),
        cover: None,
        tracks: Vec::new(),
        released: None,
        label: None,
        account: Some("Kite Theory".into()),
        about: None,
        credits: None,
        tags: Vec::new(),
    });
    let held = w.served.catalogue().record_for_tests("Night Walks", 0);
    assert!(w.served.catalogue().set_reference(&crate::serve::ids::record(held), &crate::catalogue::records::Ref::new("bandcamp", "album", page), "link").unwrap());
    let (status, view) = w.ask(&me, Method::GET, &format!("/api/albums/bandcamp/{}", crate::bandcamp::track_id(page, 0))).await;
    assert_eq!((status, view["record"].as_str()), (StatusCode::OK, Some(crate::serve::ids::record(held).as_str())), "{view}");
}

#[tokio::test]
async fn show_all_past_two_hundred_results_answers() {
    // The audit's request, on as many tracks as it takes to reach it: a fifth page of 50.
    let w = Whole::open("show-all").await;
    let cloud = |n: i64| crate::catalogue::ServedRow {
        rel: format!("The Orb/Adventures (1991)/{n:03} Cloud.flac"),
        track: Some(n),
        tags: vec![("title".into(), format!("Cloud {n}")), ("artist".into(), "The Orb".into()), ("album".into(), "Adventures".into()), ("tracknumber".into(), n.to_string())],
        ..Default::default()
    };
    let rows = (1..=260).map(cloud).collect();
    w.served.library.refresh(&[crate::serve::library::FolderRows { folder: 0, root: "/music".into(), rows }], &Default::default()).unwrap();
    let me = w.session("me");
    let (status, found) = w.ask(&me, Method::GET, "/api/search?q=cloud&kinds=track&limit=50&offset=200").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!((found["groups"]["track"]["total"].as_u64(), found["groups"]["track"]["items"].as_array().map(Vec::len)), (Some(260), Some(50)));
}

#[tokio::test]
async fn show_all_gives_every_track_once_where_the_ranking_is_not_the_index_s_order() {
    // The index's order is by how often the word comes, in how little text: an artist that says it eight times goes before a
    // long title that says it once. The ranking is the other way round: a word in the title is a direct hit.
    let w = Whole::open("show-all-mixed").await;
    let row = |n: i64, title: String, artist: &str| crate::catalogue::ServedRow {
        rel: format!("Various/Adventures (1991)/{n:03} Track.flac"),
        track: Some(n),
        tags: vec![("title".into(), title), ("artist".into(), artist.into()), ("album".into(), "Adventures".into()), ("tracknumber".into(), n.to_string())],
        ..Default::default()
    };
    let by_the_artist = (1..=230).map(|n| row(n, format!("Track {n}"), "Cloud Cloud Cloud Cloud Cloud Cloud Cloud Cloud"));
    let by_the_title = (231..=260).map(|n| row(n, format!("Cloud over the long grey water of a river with no name and no end at all {n}"), "The Orb"));
    let rows = by_the_artist.chain(by_the_title).collect();
    w.served.library.refresh(&[crate::serve::library::FolderRows { folder: 0, root: "/music".into(), rows }], &Default::default()).unwrap();
    let first = w.served.library.find(&crate::search::words("cloud"), 200).unwrap();
    assert!(first.tracks.iter().all(|t| !t.title.starts_with("Cloud")), "the case: the index's first 200 hold no direct hit");

    let me = w.session("me");
    let (mut seen, mut titles) = (Vec::new(), Vec::new());
    loop {
        let (status, found) = w.ask(&me, Method::GET, &format!("/api/search?q=cloud&kinds=track&limit=50&offset={}", seen.len())).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(found["groups"]["track"]["total"].as_u64(), Some(260));
        let page = found["groups"]["track"]["items"].as_array().unwrap().clone();
        if page.is_empty() {
            break;
        }
        seen.extend(page.iter().map(|i| i["track"].as_str().unwrap().to_string()));
        titles.extend(page.iter().map(|i| i["title"].as_str().unwrap().to_string()));
    }
    let once: std::collections::HashSet<&String> = seen.iter().collect();
    assert_eq!((seen.len(), once.len()), (260, 260), "each once, so none lost");
    assert!(titles[..30].iter().all(|t| t.starts_with("Cloud")), "the direct hits first, from the first page: {:?}", &titles[..30]);
}

#[tokio::test]
async fn sharing_playlists_in_a_batch_is_behind_the_administrators_fence() {
    let w = Whole::open("link").await;
    w.served.users.from_directory("ann", false, None).unwrap();
    let share = || Body::from(serde_json::json!({ "ids": ["Mine"], "on": true }).to_string());
    let path = "/api/batch/playlists/link";
    let (ann, me) = (w.session("ann"), w.session("me"));
    assert_eq!(w.send(("cookie", &ann), Method::POST, path, share()).await.0, StatusCode::FORBIDDEN, "an account");
    assert_eq!(w.send(("authorization", &w.key("me", "control")), Method::POST, path, share()).await.0, StatusCode::FORBIDDEN, "an administrator's key, as on the single route");
    // Past the fence, where the handler answers for each playlist.
    let (status, answer) = w.send(("cookie", &me), Method::POST, path, share()).await;
    assert_eq!((status, answer["target"].as_str()), (StatusCode::OK, Some("shared")));
}

#[tokio::test]
async fn a_group_s_sound_answers_only_a_player_of_the_group_and_nothing_else_makes_a_stream() {
    let w = Whole::open("groups").await;
    let on = w.app.play_on.clone();
    let snap = crate::snapcast::Snapcast::new(on.clone(), 1000);
    assert!(w.app.snapcast.set(snap.clone()).is_ok());
    on.serve_sync_play(1704, 1000, None);
    let (_tablet, _page, _kitchen) = (on.connect("tablet").unwrap(), on.connect_browser("page", Some("tab-1")).unwrap(), on.connect("kitchen").unwrap());
    on.command("page", crate::app::PlayOnCommand::Join { to: "tablet".into() }).await.unwrap();
    let me = w.session("me");
    // The status alone: the answer that passes never ends.
    let status = async |path: String| {
        let req = Request::builder().uri(path).header("cookie", me.as_str()).body(Body::empty()).unwrap();
        w.router.clone().oneshot(req).await.unwrap().status()
    };
    for route in ["listen.wav", "listen.pcm"] {
        assert_eq!(status(format!("/api/groups/made%20up/{route}?player=page")).await, StatusCode::NOT_FOUND, "no such group");
        assert_eq!(status(format!("/api/groups/tablet/{route}?player=kitchen")).await, StatusCode::NOT_FOUND, "a player not in it");
        assert_eq!(status(format!("/api/groups/tablet/{route}?player=made%20up")).await, StatusCode::NOT_FOUND, "no such player");
    }
    assert_eq!(snap.streams_held(), 0);
    for route in ["listen.wav", "listen.pcm"] {
        assert_eq!(status(format!("/api/groups/tablet/{route}?player=page")).await, StatusCode::OK);
    }
}

#[tokio::test]
async fn every_answer_tells_a_browser_not_to_guess_its_type() {
    let w = Whole::open("nosniff").await;
    let me = w.session("me");
    // What a route answers, a refusal, what no route takes (the page's), a file a tag could have reached: all of them.
    for (cookie, path) in [(me.as_str(), "/api/about"), ("", "/api/about"), (me.as_str(), "/"), (me.as_str(), "/api/albums/cratarr/al-1/cover")] {
        let req = Request::builder().uri(path).header("cookie", cookie).body(Body::empty()).unwrap();
        let r = w.router.clone().oneshot(req).await.unwrap();
        assert_eq!(r.headers().get("x-content-type-options").and_then(|v| v.to_str().ok()), Some("nosniff"), "{path} with {cookie:?}");
    }
}

#[tokio::test]
async fn every_route_is_behind_the_sign_in_and_is_there() {
    let w = Whole::open("sweep").await;
    let (router, cookie) = (&w.router, w.session("me"));

    let routes = named_routes();
    assert!(routes.len() > 150, "the source's routes found: {}", routes.len());
    let mut open = Vec::new();
    for (path, methods) in &routes {
        for m in methods {
            let req = Request::builder().method(m.clone()).uri(path.as_str()).header("accept", "application/json").body(Body::empty()).unwrap();
            let status = router.clone().oneshot(req).await.unwrap().status();
            if crate::serve::access::public(path) {
                open.push(path.clone());
            } else {
                assert_eq!(status, StatusCode::UNAUTHORIZED, "{m} {path} without a way in");
            }
        }
        // The guest page and its routes answer only at home (decision 0079): away, as this sweep is, nothing is there.
        if path == "/guest" || path.starts_with("/api/guest/") {
            continue;
        }
        // A method no route has: the path is there (405), not the fallback's.
        let req = Request::builder()
            .method(Method::from_bytes(b"PROPFIND").unwrap())
            .uri(path.as_str())
            .header("cookie", cookie.as_str())
            .body(Body::empty())
            .unwrap();
        let status = router.clone().oneshot(req).await.unwrap().status();
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "{path}: not in the router main serves");
    }
    open.sort();
    open.dedup();
    assert!(open.iter().all(|p| p == "/signin" || p == "/signout" || p.starts_with("/api/health") || p.starts_with("/api/auth/check") || p.starts_with("/rest/") || p.starts_with("/api/scrobble/lastfm/callback/") || p == "/api/spotify/callback" || p == "/guest" || p.starts_with("/api/guest/")), "open: {open:?}");
}

/// X5: a user may undo their own Incoming action (Add to, Delete), by the account the journal names; another's stays the
/// administrators'.
#[tokio::test]
async fn a_user_may_undo_their_own_incoming_action_and_no_one_elses() {
    let w = Whole::open("undo").await;
    for user in ["ann", "bo"] {
        w.served.users.from_directory(user, false, None).unwrap();
    }
    let journal = w.app.incoming.journal_for_tests();
    let deleted = |title: &str| crate::journal::Event::AlbumRejected { source: "cratarr".into(), album_id: "al-1".into(), title: title.into(), folder: 1, dir: "X/A".into(), token: "t".into(), release_group: None };
    let anns = crate::journal::done_by(Some("ann".into()), async { journal.record(1, &deleted("Ann's")).unwrap() }).await;
    let bos = crate::journal::done_by(Some("bo".into()), async { journal.record(1, &deleted("Bo's")).unwrap() }).await;
    let ann = w.session("ann");
    let undo = |id: i64| serde_json::json!({ "tokens": [id.to_string()] });
    assert_eq!(post(&w, &ann, "/api/undo", undo(bos)).await.0, StatusCode::FORBIDDEN, "another's");
    let (status, answer) = post(&w, &ann, "/api/undo", undo(anns)).await;
    assert_ne!(status, StatusCode::FORBIDDEN, "their own, past the fence: {answer}");
}
