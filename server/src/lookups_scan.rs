//! SCENARIOS-LOOKUPS LK.0 and LK.0b, the guards (R-H2, R-M3, R-M4, R-N2 to R-N6): every route the router registers, asked by root
//! and then by alice, each route on its own fresh copy of one data folder (R-N5), on a boot without the server's loops (R-N3: the
//! server's start, `app::start::loops`, is not called), every outside ask answered and counted by the stand-ins at `Sent::sent`
//! against the scope it is made in (R-N2: carried by `run::spawn` into every task the route's work spawns), until the scope has no
//! task alive or its deadline passes (R-N4). Each route runs in a runtime of its own, ended with it, so what its boot began ends too.
//! Run alone, by name (`--ignored lookups_scan`), so no ask in the process is any other test's.
//!
//! - **The list files** (`tests/get_lookups.txt`, `tests/write_lookups.txt`): one line a path asked, `ROUTE | PATH | ROOT'S ANSWER |
//!   STATE | NOTE` (and `| BODY` for a write). `STATE` is `-` (moved: asks nothing), `waiting <run>` (asks a source today; the run
//!   that takes it off), or `unproved <why>` (the MCP tools, until their per-request wrapper exists: R-N6). Paths name the data
//!   set's ids by `{name}` (`Ids`).
//! - **The first waiting sets** are constants (`FIRST_WAITING`, `FIRST_WAITING_WRITES`): a waiting route outside them fails, so no
//!   route goes back onto the list.
//! - **What counts:** every ask of a source `reach::name_of` knows (T-LK-6) but the exempt ones: image bytes at an address held (any
//!   route, T-LK-1), what starting the audio needs on the playback routes (T-LK-2, T-LK-12), a sign-in's own way back (T-LK-3).
//!   A non-GET's asks inside a runner job's work are the job's, counted apart (T-LK-11).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request};
use tower::ServiceExt;

use crate::app::reach::stand_ins::{Asked, StandIns};

/// LK.0's first waiting set (the 27 route patterns that asked a source under a GET at 3e02ffeff, SCENARIOS-LOOKUPS' table).
const FIRST_WAITING: [&str; 27] = [
    "GET /api/albums/{source}/{album}",
    "GET /api/track/{source}/{id}/details",
    "GET /api/preview/bandcamp/artist",
    "GET /api/bandcamp/find",
    "GET /api/cover/{source}/{id}",
    "GET /api/covers/release-group/{rg}",
    "GET /api/albums/{source}/{album}/cover",
    "GET /api/artists/about",
    "GET /api/home/artists",
    "GET /api/discover/artist",
    "GET /api/discover/artist/albums",
    "GET /api/artists/related",
    "GET /api/artists/missing",
    "GET /api/labels/{id}",
    "GET /api/search/beyond",
    "GET /api/musicbrainz/search",
    "GET /api/discover",
    "GET /api/discover/around",
    "GET /api/albums/{source}/{album}/discogs",
    "GET /api/artists/discogs",
    "GET /api/albums/{source}/{album}/references",
    "GET /api/albums/{source}/{album}/card",
    "GET /api/albums/{source}/{album}/covers",
    "GET /api/albums/{source}/{album}/release",
    "GET /api/file-changes/plans/{kind}",
    "GET /api/soulseek",
    "GET /api/spotify/callback",
];

/// LK.0b's first waiting set: the non-GETs that asked a source in their handler or in a bare task (SCENARIOS-LOOKUPS' LK.0b
/// table), the track fetch's (its release read, its check), and MCP's tools (R-N6).
const FIRST_WAITING_WRITES: [&str; 15] = [
    "POST /api/artists/open",
    "POST /api/tracks/describe",
    "POST /api/players/{name}/command",
    "POST /api/discover/want",
    "POST /api/requests/want",
    "PUT /api/albums/{source}/{album}/references",
    "POST /api/discover/around/next",
    "POST /api/spotify/sync",
    "POST /api/soulseek/approve",
    "POST /api/soulseek/{id}/now",
    "POST /api/track-search",
    "POST /api/track-search/{id}/fetch",
    "POST /api/track-intake",
    "POST /mcp",
    "GET /mcp",
];

/// How long a route's work may go on after its answer before what is still alive is named (R-N4, the builder's).
const DEADLINE: Duration = Duration::from_secs(20);
/// How long a handler may take before it is said to wait on a source (LK.0b: the stand-ins hold every answer until it answers).
const ANSWERS_WITHIN: Duration = Duration::from_secs(8);

/// A line's state.
#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Moved,
    Waiting(String),
    Unproved(String),
}

/// One line of a list file.
#[derive(Debug, Clone)]
struct Line {
    route: String,
    path: String,
    expect: u16,
    state: State,
    note: String,
    body: Option<String>,
}

/// A list file's lines.
fn lines(file: &str) -> Vec<Line> {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let f: Vec<&str> = l.splitn(6, " | ").map(str::trim).collect();
            assert!(f.len() >= 5, "{file}: a line of five fields at least: {l}");
            let state = match f[3] {
                "-" => State::Moved,
                s if s.starts_with("waiting") => State::Waiting(s["waiting".len()..].trim().to_string()),
                s if s.starts_with("unproved") => State::Unproved(s["unproved".len()..].trim().to_string()),
                s => panic!("{file}: no such state {s}: {l}"),
            };
            Line { route: f[0].to_string(), path: f[1].to_string(), expect: f[2].parse().unwrap_or_else(|_| panic!("{file}: {l}")), state, note: f[4].to_string(), body: f.get(5).map(|b| b.to_string()) }
        })
        .collect()
}

/// Every route the router registers, as `METHOD /pattern` (R-M4): each `.route(` of the source outside its tests, across lines,
/// its methods those its call names; `nest_service` (MCP's, GET and POST); and Subsonic's `/rest/{method}` as each method by name.
fn registered() -> BTreeSet<String> {
    let mut files = Vec::new();
    let mut dirs = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), Path::new(env!("CARGO_MANIFEST_DIR")).join("crates")];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                dirs.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                files.push(p);
            }
        }
    }
    let mut out = BTreeSet::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        let cut = ["#[cfg(test)]", "#[cfg(any(test, feature = \"test-support\"))]"].iter().filter_map(|m| text.find(m)).min().unwrap_or(text.len());
        let text = &text[..cut];
        let mut rest = text;
        while let Some(at) = rest.find(".route(") {
            rest = &rest[at + 7..];
            let after = rest.trim_start();
            if !after.starts_with('"') {
                continue;
            }
            let after = &after[1..];
            let Some(end) = after.find('"') else { break };
            let path = &after[..end];
            if !path.starts_with('/') {
                continue;
            }
            let call = &after[end..after[end..].find(".route(").map_or(after.len(), |n| end + n)];
            for (name, method) in [("get(", "GET"), ("post(", "POST"), ("put(", "PUT"), ("delete(", "DELETE"), ("patch(", "PATCH")] {
                let named = call.match_indices(name).any(|(i, _)| i == 0 || !call.as_bytes()[i - 1].is_ascii_alphanumeric() && call.as_bytes()[i - 1] != b'_');
                if named {
                    out.insert(format!("{method} {path}"));
                }
            }
        }
        if text.contains("nest_service(\"/mcp\"") {
            out.insert("GET /mcp".into());
            out.insert("POST /mcp".into());
        }
    }
    // Subsonic's methods by name (its one route dispatches them).
    if out.remove("GET /rest/{method}") {
        out.remove("POST /rest/{method}");
        for m in subsonic_methods() {
            out.insert(format!("GET /rest/{m}"));
        }
    }
    out
}

/// Subsonic's methods, as its dispatch names them (serve/routes.rs `handle`).
fn subsonic_methods() -> Vec<String> {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/api/src/serve/routes.rs")).unwrap();
    let from = text.find("pub async fn handle(").expect("Subsonic's dispatch");
    let body = &text[from..];
    let body = &body[..body.find("\n}\n").unwrap_or(body.len())];
    let mut out = BTreeSet::new();
    for line in body.lines().filter(|l| l.starts_with("        \"") && l.contains("=>")) {
        let arm = &line[..line.find("=>").unwrap()];
        let arm = arm.split(" if ").next().unwrap();
        for name in arm.split('|') {
            let name = name.trim().trim_matches('"');
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric()) {
                out.insert(name.to_string());
            }
        }
    }
    out.into_iter().collect()
}

/// What an ask is to the guards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Image bytes at an address held (T-LK-1).
    Image,
    /// A stream's bytes, or a Bandcamp page at a held link read to start the audio (T-LK-2): exempt on the playback routes only.
    Audio,
    /// A sign-in's way back on its own service (T-LK-3): exempt on its route only.
    SignIn,
    /// Any other: a lookup.
    Lookup,
}

fn kind_of(a: &Asked) -> Kind {
    let (host, path) = (a.host.as_str(), a.path.as_str());
    let image = match a.source {
        "coverartarchive" => path.contains("/front") || path.contains("/back"),
        "discogs" => host.starts_with("i.") || host.starts_with("img") || host.starts_with("st."),
        "bandcamp" => host.ends_with("bcbits.com") && path.starts_with("/img/"),
        "theaudiodb" => path.contains("/images/"),
        "wikimedia" => host.starts_with("upload."),
        "spotify" => host.ends_with("scdn.co") || host.ends_with("spotifycdn.com"),
        _ => false,
    };
    if image {
        return Kind::Image;
    }
    if a.source == "bandcamp" && (host.ends_with("bcbits.com") || host.starts_with("popplers") || path.starts_with("/stream_redirect") || path.starts_with("/album/") || path.starts_with("/track/") || path == "/" && host != "bandcamp.com") {
        return Kind::Audio;
    }
    if (a.source == "spotify" && (host == "accounts.spotify.com" || path == "/v1/me")) || (a.source == "lastfm" && a.query.contains("auth.getSession")) {
        return Kind::SignIn;
    }
    Kind::Lookup
}

/// Whether `kind` passes on `route` (LK.0's exempt asks).
fn exempt(route: &str, kind: Kind) -> bool {
    match kind {
        Kind::Image => true,
        Kind::Audio => {
            ["GET /api/stream/{source}/{id}", "GET /api/file/{source}/{id}", "GET /api/preview/bandcamp", "GET /api/preview/bandcamp/artist", "POST /api/players/{name}/command", "POST /api/radio/next"].contains(&route)
        }
        Kind::SignIn => ["GET /api/spotify/callback", "GET /api/scrobble/lastfm/callback/{code}"].contains(&route),
        Kind::Lookup => false,
    }
}

/// The data set's ids, by the names the list files' paths use.
type Ids = BTreeMap<&'static str, String>;

/// A server on the scan's working folder, as `main` opens one but its loops (R-N3): the process's server and journal set to it.
struct Boot {
    router: axum::Router,
    served: Arc<crate::serve::Serve>,
    app: Arc<crate::app::App>,
}

async fn boot(root: &Path) -> Boot {
    let data = root.join("data");
    let dir = data.to_string_lossy().into_owned();
    let env = |k: &str| (k == "CRATARR_DATA_DIR").then(|| dir.clone());
    let cfg = crate::config::Config::from_lookup(env).unwrap();
    let saved = crate::settings::load(&data.join("settings.json"), &env).unwrap().expect("its settings");
    let served = crate::serve::Serve::open(Some(&data), saved.tag_rules.clone(), &env).unwrap();
    crate::serve::set_instance(served.clone());
    let app = crate::app::App::new(cfg, saved, crate::settings::Origin::File, Some(served.clone())).await.unwrap();
    crate::journal::set_instance(app.journal.clone());
    served.library.set_kinds(app.library.folder_kinds());
    served.set_rules(app.library.folder_rules());
    served.attach_catalogue(app.library.catalogue());
    served.set_playlists(app.crates.clone());
    served.set_incoming(app.incoming.clone());
    served.set_play_on(app.play_on.clone());
    let router = super::routes(app.clone(), served.clone());
    Boot { router, served, app }
}

/// The scan's own folder: its working folder (one path, so what the data set stores names it), the template kept, the data set's
/// ids, and each chunk's results.
fn base() -> PathBuf {
    std::env::temp_dir().join(format!("cratarr-lookups-scan-{}", std::process::id()))
}

/// The working folder and the template, in a scan's folder.
fn folders(base: &Path) -> (PathBuf, PathBuf) {
    (base.join("work"), base.join("template"))
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let (p, q) = (e.path(), to.join(e.file_name()));
        if p.is_dir() {
            copy_dir(&p, &q);
        } else {
            std::fs::copy(&p, &q).unwrap();
        }
    }
}

/// The data set (LK.0's Given), built once in the working folder and kept as the template: a had record whose files name its
/// MusicBrainz release and artist, a record not had holding a MusicBrainz release group, a Discogs master and a Bandcamp album
/// page, an arrival in Incoming, the artist's MusicBrainz and Discogs ids, Discogs' key, a release group with no cover kept, a
/// Bandcamp album page and its preview's id never read, root (an administrator) and alice signed in.
async fn template(base: &Path) -> Ids {
    let (work, template) = folders(base);
    let _ = std::fs::remove_dir_all(base);
    let (music, incoming, data) = (work.join("music"), work.join("incoming"), work.join("data"));
    for d in [&music, &incoming, &data] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::create_dir_all(music.join(crate::playlists::PLAYLIST_DIR)).unwrap();
    let (m, i) = (music.to_string_lossy().into_owned(), incoming.to_string_lossy().into_owned());
    let kind = crate::source::cratarr::KIND.id;
    let settings = serde_json::json!({
        "version": 4,
        "music_folder": m,
        "sources": [{ "id": kind, "kind": kind, "enabled": true,
            "libraries": [{ "id": m, "kind": "main" }, { "id": i, "kind": "incoming" }], "values": { "folders": [m, i] } }],
    });
    std::fs::write(data.join("settings.json"), serde_json::to_vec_pretty(&settings).unwrap()).unwrap();
    let b = boot(&work).await;
    let served = b.served.clone();
    served.users.set("me", "pw").unwrap();
    served.users.from_directory("alice", false, None).unwrap();
    served.sessions.set_required(true).unwrap();
    let now = crate::app::moment::now_secs();
    let session = |user: &str| format!("{}={}", crate::serve::signin::COOKIE, served.sessions.create(user, "the scan", now).unwrap().0);
    let mut ids = Ids::new();
    ids.insert("root", session("me"));
    ids.insert("alice", session("alice"));
    // The had record: Night Ferry, its files naming its release and artist; and an arrival in Incoming naming none.
    const RELEASE: &str = "a1b2c3d4-0000-4000-8000-000000000101";
    const ARTIST: &str = "a1b2c3d4-0000-4000-8000-000000000102";
    const GROUP: &str = "a1b2c3d4-0000-4000-8000-000000000103";
    let cat = b.app.library.catalogue();
    let plant = |root: &Path, dir: &str, n: u32| {
        std::fs::create_dir_all(root.join(dir)).unwrap();
        for t in 1..=n {
            std::fs::write(root.join(dir).join(format!("{t:02} Track {t}.flac")), format!("{dir} {t}")).unwrap();
        }
    };
    plant(&music, "Glass Harbour/Night Ferry", 4);
    plant(&music, "Glass Harbour/Signal", 2);
    plant(&incoming, "Kite Theory/Untitled Shore", 3);
    // Their covers beside them: Home shows no album without one.
    for dir in ["Glass Harbour/Night Ferry", "Glass Harbour/Signal"] {
        std::fs::write(music.join(dir).join("cover.jpg"), b"a picture").unwrap();
    }
    cat.walk_all();
    cat.pretend_read_for_tests();
    let (music_id, incoming_id) = (cat.folders().id_of(&music).unwrap(), cat.folders().id_of(&incoming).unwrap());
    for t in 1..=4u32 {
        let (title, number) = (format!("Track {t}"), t.to_string());
        cat.tag_for_tests(music_id, &format!("Glass Harbour/Night Ferry/{t:02} Track {t}.flac"), &[("album", "Night Ferry"), ("albumartist", "Glass Harbour"), ("artist", "Glass Harbour"), ("title", title.as_str()), ("tracknumber", number.as_str()), ("musicbrainz_albumid", RELEASE), ("musicbrainz_albumartistid", ARTIST), ("musicbrainz_artistid", ARTIST), ("musicbrainz_releasegroupid", GROUP), ("genre", "Ambient"), ("date", "2012")]);
    }
    for t in 1..=2u32 {
        let (title, number) = (format!("Signal {t}"), t.to_string());
        cat.tag_for_tests(music_id, &format!("Glass Harbour/Signal/{t:02} Track {t}.flac"), &[("album", "Signal"), ("albumartist", "Glass Harbour"), ("artist", "Glass Harbour"), ("title", title.as_str()), ("tracknumber", number.as_str()), ("musicbrainz_albumartistid", ARTIST), ("musicbrainz_artistid", ARTIST)]);
    }
    for t in 1..=3u32 {
        let (title, number) = (format!("Track {t}"), t.to_string());
        cat.tag_for_tests(incoming_id, &format!("Kite Theory/Untitled Shore/{t:02} Track {t}.flac"), &[("album", "Untitled Shore"), ("albumartist", "Kite Theory"), ("artist", "Kite Theory"), ("title", title.as_str()), ("tracknumber", number.as_str())]);
    }
    cat.identify().unwrap();
    let s = served.clone();
    crate::run::spawn_blocking(move || s.refresh()).await.unwrap().unwrap();
    let had = served.library.album_in(music_id, "Glass Harbour/Night Ferry").unwrap().expect("Night Ferry built");
    let arrival = served.library.album_in(incoming_id, "Kite Theory/Untitled Shore").unwrap().expect("the arrival built");
    let track = crate::app::renumber_base::tracks_of_in(&served, &had).unwrap().0.first().map(|t| t.id.clone()).expect("a track");
    // A record not had, due its first read: its release group, a Discogs master and a Bandcamp album page.
    const PAGE: &str = "https://glassharbour.bandcamp.com/album/tide-tables";
    const NOT_HAD_GROUP: &str = "a1b2c3d4-0000-4000-8000-000000000104";
    let not_had = cat.record_for_tests("Tide Tables", 0);
    let subject = crate::serve::ids::record(not_had);
    for (source, kind, value) in [("musicbrainz", "release-group", NOT_HAD_GROUP), ("discogs", "master", "4242"), ("bandcamp", "album", PAGE)] {
        cat.set_reference(&subject, &crate::catalogue::records::Ref::new(source, kind, value), "link").unwrap();
    }
    // The artist's ids at MusicBrainz (by its files) and Discogs.
    let artist = cat.artist_of("Glass Harbour").unwrap().map(crate::serve::ids::artist).expect("Glass Harbour, an artist of the files");
    let _ = cat.set_reference(&artist, &crate::catalogue::records::Ref::new("discogs", "artist", "777"), "link");
    // Root's and alice's plays of the artist (what Discover's look and Around pick from).
    let played = cratarr_api::userdata::Annotation { play_count: 5, play_date: Some(now), own_count: 5, own_date: Some(now), ..Default::default() };
    for user in ["me", "alice"] {
        served.userdata.set_annotation(user, cratarr_api::userdata::Kind::Artist, &artist, None, &played).unwrap();
    }
    // A label with its MusicBrainz id and no rows read.
    let label = cat.label_for("Warp").unwrap();
    cat.set_reference(&crate::serve::ids::label(label), &crate::catalogue::records::Ref::new("musicbrainz", "label", "a1b2c3d4-0000-4000-8000-000000000105"), "link").unwrap();
    // Discogs' key, so its searches ask.
    served.userdata.set_import_choice(crate::app::discogs::KEY, "a-key").unwrap();
    served.userdata.set_import_choice(crate::app::discogs::SECRET, "a-secret").unwrap();
    ids.insert("had", had);
    ids.insert("arrival", arrival);
    ids.insert("not_had", subject);
    ids.insert("track", track);
    ids.insert("artist", artist);
    ids.insert("artist_mbid", ARTIST.to_string());
    ids.insert("label", crate::serve::ids::label(label));
    ids.insert("rg", NOT_HAD_GROUP.to_string());
    ids.insert("page", PAGE.to_string());
    ids.insert("bc_album", crate::bandcamp::track_id(PAGE, 0));
    ids.insert("bc_track", crate::bandcamp::track_id(PAGE, 1));
    drop(b);
    copy_dir(&work, &template);
    ids
}

/// A path with the data set's ids put in.
fn filled(path: &str, ids: &Ids) -> String {
    let mut out = path.to_string();
    for (name, value) in ids {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

/// What one account's ask of a line did: root's answer, the lookups, the job's asks, the handler's wait, the tasks still alive.
#[derive(Debug, Default)]
struct Heard {
    status: u16,
    /// The answer's first words (a stream's: none read).
    said: String,
    lookups: Vec<String>,
    jobs: Vec<String>,
    waited: bool,
    alive: usize,
}

/// One account's ask of `line` on `b`, in scope `n`: answered while every source is held (a handler that waits on one can't
/// answer in time), then let go, and settled.
async fn ask(b: &Boot, line: &Line, ids: &Ids, cookie: &str, n: u64, stand_ins: &StandIns, writes: bool) -> Heard {
    use crate::run::scope::{Scope, within};
    let (method, _) = line.route.split_once(' ').unwrap();
    let path = filled(&line.path, ids);
    let body = line.body.as_deref().map(|b| filled(b, ids)).unwrap_or_default();
    let req = Request::builder()
        .method(Method::from_bytes(method.as_bytes()).unwrap())
        .uri(&path)
        .header("cookie", cookie)
        .header("accept", "application/json")
        .header("content-type", "application/json")
        .body(if body.is_empty() { Body::empty() } else { Body::from(body) })
        .unwrap();
    let scope = Scope::new(n);
    // A non-GET answers before any source does (LK.0b): every source held until it has.
    if writes {
        for s in ["musicbrainz", "discogs", "bandcamp", "lastfm", "listenbrainz", "coverartarchive", "theaudiodb", "wikimedia", "acoustid", "spotify"] {
            stand_ins.hold(s);
        }
    }
    let router = b.router.clone();
    let mut answer = crate::run::spawn(within(scope.clone(), async move { router.oneshot(req).await }));
    let (response, waited) = match tokio::time::timeout(ANSWERS_WITHIN, &mut answer).await {
        Ok(r) => (Some(r.unwrap().unwrap()), false),
        Err(_) => {
            stand_ins.let_go();
            (tokio::time::timeout(DEADLINE, answer).await.ok().map(|r| r.unwrap().unwrap()), true)
        }
    };
    let status = response.as_ref().map_or(0, |r| r.status().as_u16());
    // Its first words, where it ends soon (a stream's never does).
    let said = match response {
        Some(r) => match tokio::time::timeout(Duration::from_millis(500), axum::body::to_bytes(r.into_body(), 1 << 20)).await {
            Ok(Ok(b)) => String::from_utf8_lossy(&b).chars().take(160).collect(),
            _ => "(a stream)".to_string(),
        },
        None => String::new(),
    };
    stand_ins.let_go();
    let until = tokio::time::Instant::now() + DEADLINE;
    while scope.alive() > 0 && tokio::time::Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut heard = Heard { status, said, waited, alive: scope.alive(), ..Heard::default() };
    for a in stand_ins.asked().into_iter().filter(|a| a.scope == Some(n)) {
        let k = kind_of(&a);
        if exempt(&line.route, k) {
            continue;
        }
        let said = format!("{} {} {}{}", a.source, a.method, a.host, a.path);
        if a.job { heard.jobs.push(said) } else { heard.lookups.push(said) }
    }
    heard
}

/// Each line's ask by each account runs in a child process of this test binary (`chunk_of_a_scan`): a route's boot leaves what it
/// opened behind (the server's parts hold each other, as they do for the life of `main`), and every in-memory cache (Bandcamp's
/// pages, Beyond's answers, the artists' looks) starts empty in a process of its own, so neither an earlier route nor root's ask
/// warms alice's (LK.0's harness 4).
const ACCOUNTS: [&str; 2] = ["root", "alice"];

/// What one account's ask heard, as a child writes it for the scan.
fn heard_json(h: &Heard) -> serde_json::Value {
    serde_json::json!({ "status": h.status, "said": h.said, "lookups": h.lookups, "jobs": h.jobs, "waited": h.waited, "alive": h.alive })
}

fn heard_of(v: &serde_json::Value) -> Heard {
    let list = |k: &str| v[k].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(str::to_string)).collect();
    Heard { status: v["status"].as_u64().unwrap_or(0) as u16, said: v["said"].as_str().unwrap_or_default().to_string(), lookups: list("lookups"), jobs: list("jobs"), waited: v["waited"].as_bool().unwrap_or(false), alive: v["alive"].as_u64().unwrap_or(0) as usize }
}

/// The scan of one list file: the template built here, then each line asked by root and by alice, each ask in a child process of
/// its own (`ACCOUNTS`) on a fresh copy of the template. The survey of every line is written beside the list file
/// (`<file>.out`, never committed), and what fails is said at the end.
fn scan(file: &str, first_waiting: &[&str], writes: bool) {
    let all = lines(file);
    let routes = registered();
    let mine: BTreeSet<String> = routes.iter().filter(|r| r.starts_with("GET ") != writes).cloned().collect();
    let listed: BTreeSet<String> = all.iter().map(|l| l.route.clone()).collect();
    let mut failed: Vec<String> = Vec::new();
    for r in mine.difference(&listed) {
        failed.push(format!("{r}: registered, not in {file}"));
    }
    for r in listed.difference(&mine) {
        failed.push(format!("{r}: in {file}, not registered"));
    }
    for l in &all {
        if let State::Waiting(_) = l.state
            && !first_waiting.contains(&l.route.as_str())
        {
            failed.push(format!("{}: waiting, but not in the first waiting set", l.route));
        }
    }
    let base = base().join(if writes { "writes" } else { "gets" });
    crate::app::reach::stand_ins::give_everything();
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    let ids = rt.block_on(async {
        let _stand_ins = StandIns::set(Arc::new(answers)).await;
        template(&base).await
    });
    rt.shutdown_timeout(Duration::from_secs(5));
    std::fs::write(base.join("ids.json"), serde_json::to_vec(&ids).unwrap()).unwrap();
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(format!("{file}.out"));
    let started = std::time::Instant::now();
    let mut survey = String::new();
    for (n, l) in all.iter().enumerate() {
        let mut heard = Vec::new();
        let mut unscoped: Vec<String> = Vec::new();
        for who in ACCOUNTS {
            let results = base.join(format!("line-{n}-{who}.json"));
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--ignored", "--exact", "lookups_scan::chunk_of_a_scan", "--nocapture", "--test-threads=1"])
                .env("LOOKUPS_SCAN", format!("{file}|{n}|{who}|{writes}|{}", base.display()))
                .status()
                .unwrap();
            let v: serde_json::Value = std::fs::read(&results).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
            if !status.success() || v.is_null() {
                failed.push(format!("{} {}: {who}'s child ended {status} with nothing heard", l.route, l.path));
            }
            heard.push(heard_of(&v["heard"]));
            unscoped.extend(v["unscoped"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(str::to_string)));
        }
        let (root, alice) = (&heard[0], &heard[1]);
        survey.push_str(&format!(
            "{} | {} | root {} alice {} | lookups {:?} {:?} | jobs {:?} {:?} | waited {} {} | alive {} {} | unscoped {:?} | said {:?} {:?}\n",
            l.route, l.path, root.status, alice.status, root.lookups, alice.lookups, root.jobs, alice.jobs, root.waited, alice.waited, root.alive, alice.alive, unscoped, root.said, alice.said
        ));
        judge(l, root, alice, &unscoped, writes, &mut failed);
        // Kept as it goes: a scan stopped part way still says how far it got, and how long it took.
        let _ = std::fs::write(&out, format!("{survey}# {} of {} after {} s\n", n + 1, all.len(), started.elapsed().as_secs()));
    }
    let _ = std::fs::write(&out, &survey);
    let _ = std::fs::remove_dir_all(&base);
    assert!(failed.is_empty(), "{} failed (survey: {}):\n{}", failed.len(), out.display(), failed.join("\n"));
}

/// One line's verdict (LK.0's and LK.0b's Then): root's answer the list's; a moved line asks nothing (a GET nothing at all, a job's
/// included; a non-GET nothing outside its jobs), waits on no source and leaves no task alive; a waiting line still asks; no ask
/// outside every route's scope.
fn judge(l: &Line, root: &Heard, alice: &Heard, unscoped: &[String], writes: bool, failed: &mut Vec<String>) {
    let at = format!("{} {}", l.route, l.path);
    if !unscoped.is_empty() {
        failed.push(format!("{at}: an ask outside every route's scope: {unscoped:?}"));
    }
    if let State::Unproved(_) = l.state {
        return;
    }
    if root.status != l.expect {
        failed.push(format!("{at}: root's answer {} where the list says {}", root.status, l.expect));
    }
    let counted = |h: &Heard| -> Vec<String> { if writes { h.lookups.clone() } else { h.lookups.iter().chain(&h.jobs).cloned().collect() } };
    let (by_root, by_alice) = (counted(root), counted(alice));
    let asked = by_root.len() + by_alice.len();
    match &l.state {
        State::Moved => {
            if asked > 0 {
                failed.push(format!("{at}: asks {by_root:?} {by_alice:?}"));
            }
            if writes && (root.waited || alice.waited) {
                failed.push(format!("{at}: waits on a source before it answers"));
            }
            for (who, h) in [("root", root), ("alice", alice)] {
                if h.alive > 0 {
                    failed.push(format!("{at}: {} of {who}'s tasks alive past the deadline", h.alive));
                }
            }
        }
        State::Waiting(run) if asked == 0 => failed.push(format!("{at}: waiting for {run} ({}), but asks nothing: take it off", l.note)),
        _ => {}
    }
}

/// One line's ask by one account, in a process of its own (`scan` starts it, naming the list, the line, the account and the scan's
/// folder in `LOOKUPS_SCAN`): on a fresh copy of the template, in a runtime of its own, what it heard written for the scan. Without
/// `LOOKUPS_SCAN` it does nothing.
#[test]
#[ignore = "one line of LK.0's or LK.0b's scan, started by the scan in a process of its own"]
fn chunk_of_a_scan() {
    let Ok(spec) = std::env::var("LOOKUPS_SCAN") else { return };
    crate::app::reach::stand_ins::give_everything();
    let f: Vec<&str> = spec.splitn(5, '|').collect();
    let (file, n, who, writes, base) = (f[0], f[1].parse::<usize>().unwrap(), f[2], f[3] == "true", PathBuf::from(f[4]));
    let l = &lines(file)[n];
    let ids: Ids = {
        let read: BTreeMap<String, String> = serde_json::from_slice(&std::fs::read(base.join("ids.json")).unwrap()).unwrap();
        read.into_iter().map(|(k, v)| (&*Box::leak(k.into_boxed_str()), v)).collect()
    };
    let (work, template) = folders(&base);
    let _ = std::fs::remove_dir_all(&work);
    copy_dir(&template, &work);
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    let (heard, stand_ins) = rt.block_on(async {
        let stand_ins = StandIns::set(Arc::new(answers)).await;
        let b = boot(&work).await;
        stand_ins.clear();
        let id = n as u64 * 2 + u64::from(who == "alice") + 2;
        (ask(&b, l, &ids, &ids[who], id, &stand_ins, writes).await, stand_ins)
    });
    let unscoped: Vec<String> = stand_ins.asked().iter().filter(|a| a.scope.is_none() && kind_of(a) == Kind::Lookup).map(|a| format!("{} {}{}", a.source, a.host, a.path)).collect();
    std::fs::write(base.join(format!("line-{n}-{who}.json")), serde_json::json!({ "heard": heard_json(&heard), "unscoped": unscoped }).to_string()).unwrap();
    drop(stand_ins);
    rt.shutdown_timeout(Duration::from_secs(5));
}

/// The stand-ins' answers: a small fixed answer for each kind of ask, so a chain goes on far enough to be counted.
fn answers(a: &Asked) -> Option<(u16, String)> {
    Some(match (a.source, a.path.as_str()) {
        ("musicbrainz", p) if p.starts_with("/ws/2/release-group") => (200, r#"{"release-groups":[],"releases":[],"relations":[]}"#.into()),
        ("musicbrainz", _) => (200, r#"{"releases":[],"artists":[],"release-groups":[],"labels":[],"relations":[],"recordings":[]}"#.into()),
        ("discogs", _) => (200, r#"{"results":[],"releases":[],"tracklist":[],"pagination":{"pages":1}}"#.into()),
        ("bandcamp", _) => (404, String::new()),
        ("coverartarchive", _) => (404, String::new()),
        ("listenbrainz", _) => (200, "[]".into()),
        ("lastfm", _) => (200, r#"{"similarartists":{"artist":[]},"toptags":{"tag":[]}}"#.into()),
        ("theaudiodb", _) => (200, r#"{"artists":null}"#.into()),
        ("wikimedia", _) => (200, r#"{"entities":{}}"#.into()),
        ("acoustid", _) => (200, r#"{"status":"ok","results":[]}"#.into()),
        _ => (404, String::new()),
    })
}

/// LK.0: no GET asks an outside source; the exempt asks are named. Every GET route the router registers (each Subsonic method by
/// name), asked by root and by alice on a fresh copy of the data set, counts no lookup, but the routes still waiting (each within
/// `FIRST_WAITING`, each still asking); a route the list file doesn't name fails; root's answer is the list's; an ask outside every
/// route's scope fails; no task alive at a route's deadline. Red first: with an empty waiting set, the 27 routes asked.
#[test]
#[ignore = "LK.0's scan: every route on its own data folder, alone in its process (run by name)"]
fn no_get_asks_an_outside_source() {
    scan("get_lookups.txt", &FIRST_WAITING, false);
}

/// LK.0b: no non-GET asks a source but inside a runner job, and none waits on one. Every POST, PUT, DELETE and PATCH the router
/// registers, with its body, asked by root and by alice on a fresh copy of the data set while every source is held: no inline
/// lookup, an answer before any source answers, but the routes still waiting; asks inside the jobs they begin are counted apart;
/// MCP's tools unproved until their per-request wrapper exists (R-N6).
#[test]
#[ignore = "LK.0b's scan: every route on its own data folder, alone in its process (run by name)"]
fn no_write_asks_a_source_but_in_a_job() {
    scan("write_lookups.txt", &FIRST_WAITING_WRITES, true);
}
