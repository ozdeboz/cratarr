//! What Home Assistant reads of Cratarr, pinned at 0.17's shape (the contract the Home Assistant session gathered from the Pi's
//! configuration and cratarr-ha 0.5: the Play On push to its webhook, the player commands its script sends and reads the status and
//! error of, the library calls, the health check, and the integration's own routes). Each field it reads is there, of the type it
//! reads it as; Cratarr's own ids are `tr-`/`al-`. A change that fails this breaks Home Assistant: keep the field, or change it in
//! step with the Home Assistant session.

use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use tokio_stream::StreamExt;

use crate::whole_router::Whole;

/// What a field must be, as Home Assistant reads it.
#[derive(Debug, Clone, Copy)]
enum Is {
    Str,
    Num,
    Bool,
    Arr,
    /// Null where not known.
    StrOrNull,
    NumOrNull,
    ObjOrNull,
}

impl Is {
    fn of(self, v: &Value) -> bool {
        match self {
            Is::Str => v.is_string(),
            Is::Num => v.is_number(),
            Is::Bool => v.is_boolean(),
            Is::Arr => v.is_array(),
            Is::StrOrNull => v.is_string() || v.is_null(),
            Is::NumOrNull => v.is_number() || v.is_null(),
            Is::ObjOrNull => v.is_object() || v.is_null(),
        }
    }
}

/// Each of `fields` in `v`, of its type.
fn holds(what: &str, v: &Value, fields: &[(&str, Is)]) {
    for (name, is) in fields {
        let Some(f) = v.get(name) else { panic!("{what}: Home Assistant reads `{name}`, which is gone: {v}") };
        assert!(is.of(f), "{what}: Home Assistant reads `{name}` as {is:?}: {f} in {v}");
    }
}

/// A player as the push, `GET /api/players` and its stream carry it: every field Home Assistant's automations and cratarr-ha read.
const PLAYER: &[(&str, Is)] = &[
    ("name", Is::Str),
    ("online", Is::Bool),
    ("state", Is::Str),
    ("playing", Is::Bool),
    ("title", Is::StrOrNull),
    ("artist", Is::StrOrNull),
    ("album", Is::StrOrNull),
    ("album_artist", Is::StrOrNull),
    ("index", Is::Num),
    ("position_ms", Is::Num),
    ("position_at", Is::Num),
    ("duration_ms", Is::NumOrNull),
    ("volume", Is::Num),
    ("ducked", Is::Bool),
    ("held", Is::StrOrNull),
    ("queue_length", Is::Num),
    ("queue_version", Is::Num),
    ("seen", Is::Num),
    ("repeat", Is::Str),
    ("shuffled", Is::Bool),
    ("radio", Is::Bool),
    ("radio_by", Is::Str),
    ("radio_count", Is::Num),
    ("autoplay", Is::ObjOrNull),
    ("track_id", Is::StrOrNull),
    ("album_id", Is::StrOrNull),
    ("track", Is::ObjOrNull),
    // cratarr-ha's: what it plays on, its group, its queues, its last announcement, Radio's and Autoplay's switch.
    ("kind", Is::Str),
    ("group", Is::StrOrNull),
    ("queues", Is::Arr),
    ("skip_favourites", Is::Bool),
];

/// The track a player plays (`t` in Home Assistant's templates).
const TRACK: &[(&str, Is)] = &[("id", Is::Str), ("album_id", Is::StrOrNull), ("source", Is::Str), ("name", Is::Str), ("artist", Is::StrOrNull), ("album", Is::StrOrNull)];

/// Salon as a list of players holds it, its every field read, its ids the library's: `tr-`/`al-` at the top and in its track, and
/// its track the one added by hand (`lane` "next").
fn salon_holds(what: &str, players: &Value, track: &str, album: &str) {
    let list = players.as_array().unwrap_or_else(|| panic!("{what}: `players` is a list: {players}"));
    let salon = list.iter().find(|p| p["name"] == "Salon").unwrap_or_else(|| panic!("{what}: Salon is listed: {players}"));
    holds(what, salon, PLAYER);
    holds(&format!("{what}, its track"), &salon["track"], TRACK);
    holds(&format!("{what}, its autoplay"), &salon["autoplay"], &[("mode", Is::Str), ("albums", Is::Bool)]);
    for q in salon["queues"].as_array().unwrap() {
        holds(&format!("{what}, its queues"), q, &[("id", Is::Str), ("name", Is::Str), ("playing", Is::Bool)]);
    }
    assert_eq!((salon["track_id"].as_str(), salon["album_id"].as_str()), (Some(track), Some(album)), "{what}: the library's ids: {salon}");
    let t = &salon["track"];
    assert_eq!((t["source"].as_str(), t["id"].as_str(), t["album_id"].as_str(), t["lane"].as_str()), (Some("cratarr"), Some(track), Some(album), Some("next")), "{what}: {t}");
    assert!(track.starts_with("tr-") && album.starts_with("al-"), "{track} {album}");
}

/// `path` asked with Home Assistant's key (a control key of the first administrator's, decision 0043), `body` as JSON.
async fn ha(w: &Whole, key: &str, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let body = body.map_or_else(Body::empty, |b| Body::from(b.to_string()));
    w.send(("authorization", key), method, path, body).await
}

/// The first `players` event of `GET /api/events/players`, as cratarr-ha reads the stream: its JSON.
async fn first_pushed(w: &Whole, key: &str) -> Value {
    let req = axum::http::Request::builder().uri("/api/events/players").header("authorization", key).header("accept", "text/event-stream").body(Body::empty()).unwrap();
    let r = tower::ServiceExt::oneshot(w.router.clone(), req).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK, "the players stream");
    let mut body = r.into_body().into_data_stream();
    let mut text = String::new();
    let read = async {
        loop {
            let data = body.next().await.expect("the stream goes on").unwrap();
            text.push_str(&String::from_utf8_lossy(&data));
            // One event: its lines, then a blank one.
            if let Some(end) = text.find("\n\n") {
                let event = &text[..end];
                if event.lines().any(|l| l == "event: players") {
                    let data: String = event.lines().filter_map(|l| l.strip_prefix("data:")).map(|d| d.strip_prefix(' ').unwrap_or(d)).collect();
                    return serde_json::from_str::<Value>(&data).unwrap();
                }
                text = text[end + 2..].to_string();
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(10), read).await.expect("the players event at once")
}

/// The path a webhook's capture server takes Play On's push on (a constant: the route sweep reads the source's routes as written).
const HOOK: &str = "/hook";

#[tokio::test(flavor = "multi_thread")]
async fn home_assistant_still_reads_every_field_it_uses() {
    let w = Whole::open("home-assistant").await;
    let key = w.key("me", "control");
    let key = key.as_str();

    // The library: one record of two tracks, in Music's files and Cratarr's own library, as the players and the library calls name
    // them.
    let cat = w.served.catalogue().clone();
    let record = cat.record_for_tests("Kind of Blue", 2);
    cat.copy_for_tests(record, 0);
    let tracks: Vec<i64> = cat.writer_for_tests().prepare("SELECT id FROM tracks WHERE record = ?1 ORDER BY number").unwrap().query_map([record], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    let row = |n: usize, title: &str| crate::catalogue::ServedRow {
        rel: format!("Miles Davis/Kind of Blue (1959)/0{} {title}.flac", n + 1),
        track: Some(tracks[n]),
        born: Some(1_700_000_000),
        format: Some("FLAC".into()),
        duration_ms: Some(540_000),
        size: 50_000_000,
        mtime: 1,
        tags: [("title", title), ("artist", "Miles Davis"), ("albumartist", "Miles Davis"), ("album", "Kind of Blue"), ("tracknumber", &(n + 1).to_string()), ("date", "1959"), ("genre", "Jazz")]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        ..Default::default()
    };
    let rows = vec![row(0, "So What"), row(1, "Freddie Freeloader")];
    w.served.library.refresh(&[crate::serve::library::FolderRows { folder: 0, root: "/music".into(), rows }], &Default::default()).unwrap();
    let ids = w.served.library.song_ids(tracks[1]).unwrap().expect("Freddie Freeloader in Cratarr's own library");
    let (so_what, freddie, album) = (crate::serve::ids::song(tracks[0]), ids.song_id.clone(), ids.album_id.clone());
    let queued = |id: &str, name: &str| json!({ "source": "cratarr", "id": id, "name": name, "artist": "Miles Davis", "album": "Kind of Blue", "album_id": album });

    // Home Assistant's webhook: a server of the test's own, each push it is sent kept.
    let (pushed, mut pushes) = tokio::sync::mpsc::unbounded_channel::<Value>();
    let hook = axum::Router::new().route(
        HOOK,
        axum::routing::post(move |axum::Json(v): axum::Json<Value>| {
            let pushed = pushed.clone();
            async move {
                pushed.send(v).ok();
                StatusCode::OK
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let hook_url = format!("http://{}{HOOK}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, hook).await.unwrap() });

    // Salon and Kitchen, the administrator's devices, each listening for what it is told; sync play served, as for a group.
    let on = w.app.play_on.clone();
    on.serve_sync_play(1704, 1000, None);
    for name in ["Salon", "Kitchen"] {
        let mut told = on.connect(name).unwrap();
        on.own(name, "me");
        tokio::spawn(async move { while told.next().await.is_some() {} });
    }
    on.set_webhooks_for_tests(vec![hook_url]);

    // Salon plays the album, then a track added by hand (its lane "next"), Autoplay on: as Home Assistant's script and cratarr-ha ask.
    let command = |c: Value| ha(&w, key, Method::POST, "/api/players/Salon/command", Some(c));
    let (status, answer) = command(json!({ "command": "play", "tracks": [queued(&so_what, "So What")] })).await;
    assert!(status.is_success(), "play: {status} {answer}");
    let (status, answer) = command(json!({ "command": "add", "tracks": [queued(&freddie, "Freddie Freeloader")], "how": "next" })).await;
    assert!(status.is_success(), "add: {status} {answer}");
    let (status, answer) = command(json!({ "command": "autoplay", "mode": "similar", "albums": true })).await;
    assert!(status.is_success(), "autoplay: {status} {answer}");
    let version = on.get("Salon").unwrap().queue_version;
    let report = crate::app::PlayerReport { index: 1, version: Some(version), position_ms: 12_000, duration_ms: Some(540_000), playing: true, volume: 80, ..Default::default() };
    on.report("Salon", report).unwrap();

    // 1. The push: {seq, players}, Salon in it with every field, its ids the library's.
    let push = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let p = pushes.recv().await.expect("the webhook pushed");
            let salon = p["players"].as_array().and_then(|l| l.iter().find(|s| s["name"] == "Salon").cloned());
            if salon.is_some_and(|s| s["track_id"] == freddie.as_str() && s["album_id"] == album.as_str() && s["autoplay"].is_object()) {
                return p;
            }
        }
    })
    .await
    .expect("a push of Salon playing Freddie Freeloader");
    holds("the push", &push, &[("seq", Is::Num), ("players", Is::Arr)]);
    salon_holds("the push", &push["players"], &freddie, &album);

    // 2. The fallback every 5 minutes, the player by name, and cratarr-ha's stream: the same player.
    let (status, players) = ha(&w, key, Method::GET, "/api/players", None).await;
    assert_eq!(status, StatusCode::OK, "{players}");
    salon_holds("GET /api/players", &players["players"], &freddie, &album);
    let (status, salon) = ha(&w, key, Method::GET, "/api/players/Salon", None).await;
    assert_eq!(status, StatusCode::OK, "{salon}");
    salon_holds("GET /api/players/Salon", &json!([salon.clone()]), &freddie, &album);
    let streamed = first_pushed(&w, key).await;
    salon_holds("GET /api/events/players", &streamed["players"], &freddie, &album);
    let queue = salon["queues"][0]["id"].as_str().expect("a queue to take up").to_string();

    // 3. The commands Home Assistant's script and cratarr-ha send, with their arguments as they send them: each answered with a
    // success; the script reads the status, and the error where there is one.
    let commands = [
        json!({ "command": "pause" }),
        json!({ "command": "resume" }),
        json!({ "command": "toggle" }),
        json!({ "command": "toggle" }),
        json!({ "command": "seek", "position_ms": 60_000 }),
        json!({ "command": "volume", "level": 40 }),
        json!({ "command": "duck" }),
        json!({ "command": "unduck" }),
        json!({ "command": "shuffle", "on": true }),
        json!({ "command": "repeat", "mode": "all" }),
        json!({ "command": "repeat", "mode": "off" }),
        json!({ "command": "radio", "on": true, "by": "artist" }),
        json!({ "command": "radio", "on": true, "count": 2 }),
        json!({ "command": "radio", "on": false }),
        json!({ "command": "autoplay", "mode": "similar", "restart": true }),
        json!({ "command": "autoplay", "mode": "stop" }),
        json!({ "command": "skip_favourites", "on": true }),
        json!({ "command": "skip_favourites", "on": false }),
        json!({ "command": "add", "tracks": [queued(&so_what, "So What")] }),
        json!({ "command": "move", "from": 2, "to": 0 }),
        json!({ "command": "remove", "index": 0 }),
        json!({ "command": "jump", "index": 0 }),
        json!({ "command": "next" }),
        json!({ "command": "previous" }),
        json!({ "command": "queue", "id": queue }),
        json!({ "command": "announce", "url": "http://192.168.1.20:8123/api/tts_proxy/x.mp3", "id": "ha-0123456789ab" }),
        json!({ "command": "join", "to": "Kitchen" }),
        json!({ "command": "leave" }),
        json!({ "command": "handover", "to": "Kitchen" }),
        json!({ "command": "play", "album": album, "shuffle": true }),
        json!({ "command": "stop" }),
    ];
    for c in commands {
        let to = if c["command"] == "play" || c["command"] == "stop" { "Kitchen" } else { "Salon" };
        let (status, answer) = ha(&w, key, Method::POST, &format!("/api/players/{to}/command"), Some(c.clone())).await;
        assert!(status.is_success(), "{c} to {to}: {status} {answer}");
    }
    // Refused: a status, and the error in words.
    let (status, refused) = ha(&w, key, Method::POST, "/api/players/Nowhere/command", Some(json!({ "command": "resume" }))).await;
    assert!(status.is_client_error() && refused["error"].is_string(), "a player not there: {status} {refused}");

    // 4. The library calls, by Cratarr's own ids.
    let (status, a) = ha(&w, key, Method::GET, &format!("/api/library/albums/{album}"), None).await;
    assert_eq!(status, StatusCode::OK, "{a}");
    holds("GET /api/library/albums/{al}", &a, &[("id", Is::Str), ("name", Is::Str), ("album_artist", Is::Str), ("tags", Is::Arr), ("review", Is::StrOrNull)]);
    assert_eq!(a["id"], album.as_str());
    let (status, added) = ha(&w, key, Method::POST, &format!("/api/library/albums/{album}/tags/gig"), None).await;
    assert_eq!(status, StatusCode::OK, "{added}");
    holds("POST …/tags/{tag}", &added, &[("added", Is::Bool), ("name", Is::Str)]);
    let (status, removed) = ha(&w, key, Method::DELETE, &format!("/api/library/albums/{album}/tags/gig"), None).await;
    assert_eq!(status, StatusCode::OK, "{removed}");
    holds("DELETE …/tags/{tag}", &removed, &[("removed", Is::Bool), ("name", Is::Str)]);
    ha(&w, key, Method::POST, &format!("/api/library/albums/{album}/tags/summer"), None).await;
    let (status, cleared) = ha(&w, key, Method::DELETE, &format!("/api/library/albums/{album}/tags"), None).await;
    assert_eq!(status, StatusCode::OK, "{cleared}");
    holds("DELETE …/tags", &cleared, &[("removed", Is::Arr), ("name", Is::Str)]);
    // An album of the library is no review's: refused, with its words.
    for (method, review) in [(Method::POST, "promote"), (Method::POST, "reject"), (Method::DELETE, "review")] {
        let (status, refused) = ha(&w, key, method.clone(), &format!("/api/library/albums/{album}/{review}"), None).await;
        assert!(status == StatusCode::CONFLICT && refused["error"].is_string(), "{method} …/{review}: {status} {refused}");
    }
    let (status, incoming) = ha(&w, key, Method::GET, "/api/library/incoming", None).await;
    assert_eq!(status, StatusCode::OK, "{incoming}");
    holds("GET /api/library/incoming", &incoming, &[("albums", Is::Arr)]);
    let (status, t) = ha(&w, key, Method::GET, &format!("/api/library/tracks/{freddie}"), None).await;
    assert_eq!(status, StatusCode::OK, "{t}");
    holds(
        "GET /api/library/tracks/{tr}",
        &t,
        &[("id", Is::Str), ("title", Is::Str), ("artist", Is::StrOrNull), ("album", Is::StrOrNull), ("album_id", Is::Str), ("album_artist", Is::StrOrNull), ("year", Is::NumOrNull), ("genres", Is::Arr), ("favourite", Is::Bool)],
    );
    assert_eq!((t["id"].as_str(), t["album_id"].as_str()), (Some(freddie.as_str()), Some(album.as_str())));
    for method in [Method::POST, Method::DELETE] {
        let (status, answer) = ha(&w, key, method.clone(), &format!("/api/library/tracks/{freddie}/favourite"), None).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{method} …/favourite: {answer}");
    }
    let (status, created) = ha(&w, key, Method::POST, "/api/playlists", Some(json!({ "name": "Evening" }))).await;
    assert!(status.is_success(), "{created}");
    // A playlist by its spoken name is read from a connected server, which this harness has none of (Cratarr's own source reads the
    // process's one library, `serve::set_instance`): the route answers with its error here, and what it answers with one is pinned
    // as the value it serves.
    let (status, refused) = ha(&w, key, Method::GET, "/api/playlists/by-name?name=evening&shuffle=true&limit=50", None).await;
    assert!(status == StatusCode::CONFLICT && refused["error"].is_string(), "GET /api/playlists/by-name with no server: {status} {refused}");
    let by_name = serde_json::to_value(crate::app::ByName { playlist: Some("Evening".into()), tracks: vec![freddie.clone()] }).unwrap();
    holds("GET /api/playlists/by-name", &by_name, &[("playlist", Is::StrOrNull), ("tracks", Is::Arr)]);

    // 5. The health check: `status` "ok", with no key at all.
    let (status, health) = w.send(("x-none", ""), Method::GET, "/api/health", Body::empty()).await;
    assert_eq!((status, health["status"].as_str()), (StatusCode::OK, Some("ok")), "{health}");

    // 6. cratarr-ha's own: the playlists it offers, Music's albums, the covers, the weather it tells.
    let (status, state) = ha(&w, key, Method::GET, "/api/state", None).await;
    assert_eq!(status, StatusCode::OK, "{state}");
    holds("GET /api/state", &state, &[("playlists", Is::Arr)]);
    let evening = state["playlists"].as_array().unwrap().iter().find(|p| p["name"] == "Evening").unwrap_or_else(|| panic!("{state}"));
    holds("GET /api/state, a playlist", evening, &[("id", Is::Str), ("name", Is::Str)]);
    let (status, collection) = ha(&w, key, Method::GET, "/api/collection", None).await;
    assert_eq!(status, StatusCode::OK, "{collection}");
    holds("GET /api/collection", &collection, &[("sources", Is::Arr)]);
    // Music's albums come from a connected server too (as by-name's): the route's answer as it holds one, pinned as the value it serves.
    let album_of = crate::source::Album { id: album.clone(), title: "Kind of Blue".into(), artist: Some("Miles Davis".into()), year: Some(1959), added: Some(1_700_000_000), ..Default::default() };
    let source = cratarr_base::app::collection::CollectionSourceView {
        id: "cratarr".into(),
        label: "Cratarr".into(),
        supported: true,
        albums: vec![album_of],
        error: None,
        as_of: 1_700_000_000,
        libraries: vec!["/music".into()],
    };
    let served = serde_json::to_value(crate::app::CollectionView { sources: vec![source] }).unwrap();
    assert!(!served["sources"].as_array().unwrap().is_empty());
    for source in collection["sources"].as_array().unwrap().iter().chain(served["sources"].as_array().unwrap()) {
        holds("GET /api/collection, a source", source, &[("id", Is::Str), ("albums", Is::Arr)]);
        for a in source["albums"].as_array().unwrap() {
            holds("GET /api/collection, an album", a, &[("id", Is::Str), ("title", Is::Str), ("artist", Is::StrOrNull), ("year", Is::NumOrNull), ("added", Is::NumOrNull)]);
        }
    }
    // A cover: the picture, or none (404); never a refusal or another route's.
    for path in [format!("/api/cover/cratarr/{freddie}"), format!("/api/albums/cratarr/{album}/cover")] {
        let (status, _) = ha(&w, key, Method::GET, &path, None).await;
        assert!(status == StatusCode::OK || status == StatusCode::NOT_FOUND, "{path}: {status}");
    }
    let weather = json!({ "tz": 3600, "condition": "rainy", "cloud": 90.0, "temperature": 11.5, "sun": -3.2 });
    let (status, answer) = ha(&w, key, Method::POST, "/api/home/weather", Some(weather)).await;
    assert!(status.is_success(), "POST /api/home/weather: {status} {answer}");
}
