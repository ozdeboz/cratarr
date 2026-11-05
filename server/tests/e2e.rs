//! The end-to-end suite (decision 0093): the real server binary on a fresh copy of the fixture library and a fresh data
//! folder, driven through its API as the page drives it. Types into the files (decision 0092): the catalogue read, the
//! review sheet, a run written and read back from the files, "changed since it was planned", Undo, and the run kept
//! across a restart. Everything is thrown away after. The Dockerfile's test and `e2e` stages run them by name, so a test
//! here ends its name in `end_to_end` or the gate never runs it: `cargo test --workspace -- --ignored fixtures:: end_to_end`.
//!
//! `Server` calls as nobody, which a server with no accounts takes for its administrator. `Server::sign_in` gives a
//! `Caller` that is an account, `settings` writes 0.16's settings for the whole layout (`--make-layout`), `settings_0_15`
//! what 0.15 left.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_cratarr-server");

struct Server {
    child: Child,
    base: String,
    http: reqwest::Client,
}

impl Server {
    /// The server on `data`, on a free port, once it answers: asking for a sign-in is an answer.
    async fn start(data: &Path) -> Server {
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let child = Command::new(BIN)
            .env("CRATARR_DATA_DIR", data)
            .env("CRATARR_BIND", format!("127.0.0.1:{port}"))
            .env("RUST_LOG", "warn")
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("the server starts");
        let s = Server { child, base: format!("http://127.0.0.1:{port}"), http: reqwest::Client::new() };
        let answered = |r: reqwest::Response| r.status().is_success() || r.status() == reqwest::StatusCode::UNAUTHORIZED;
        s.until("the server answers", Duration::from_secs(60), || async { s.http.get(format!("{}/api/albums", s.base)).send().await.is_ok_and(answered) }).await;
        s
    }

    /// Calls as nobody.
    fn anyone(&self) -> Caller<'_> {
        Caller { server: self, session: None }
    }

    /// Signs in as the page's form does: the answer sends a browser on, with the session in its cookie.
    async fn sign_in(&self, name: &str, password: &str) -> Caller<'_> {
        let form = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().unwrap();
        let r = form.post(format!("{}/signin", self.base)).form(&[("username", name), ("password", password)]).send().await.unwrap();
        let cookies: Vec<&str> = r.headers().get_all(reqwest::header::SET_COOKIE).iter().filter_map(|c| c.to_str().ok()?.split(';').next()).collect();
        assert!(!cookies.is_empty(), "{name} not signed in: {}", r.status());
        Caller { server: self, session: Some(cookies.join("; ")) }
    }

    async fn get(&self, path: &str) -> Value {
        self.anyone().get(path).await
    }

    /// A call that may be refused: (status, answer).
    async fn send(&self, method: reqwest::Method, path: &str, body: Value) -> (u16, Value) {
        self.anyone().send(method, path, body).await
    }

    async fn ok(&self, method: reqwest::Method, path: &str, body: Value) -> Value {
        self.anyone().ok(method, path, body).await
    }

    async fn until<F, Fut>(&self, what: &str, within: Duration, mut f: F)
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let start = Instant::now();
        while !f().await {
            assert!(start.elapsed() < within, "waited {within:?} for {what}");
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    /// The fixture library's albums as the Library lists them.
    async fn albums(&self) -> Vec<Value> {
        self.anyone().albums().await
    }

    /// The newest run once it is done and kept, other than `not`.
    async fn done_run(&self, not: Option<&str>) -> Value {
        let start = Instant::now();
        loop {
            let runs = self.get("/api/file-changes/runs").await;
            let newest = runs.as_array().and_then(|r| r.first().cloned()).unwrap_or(Value::Null);
            let id = newest["id"].as_str().unwrap_or("");
            if newest["state"] == "done" && id.starts_with('r') && Some(id) != not {
                return newest;
            }
            assert!(start.elapsed() < Duration::from_secs(120), "waited for a run to finish: {runs}");
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    /// Undoes a run and waits for its Undo to finish (it answers at once and works in the background): the run then.
    async fn undo(&self, id: &str) -> (u16, Value) {
        let (status, first) = self.send(reqwest::Method::POST, &format!("/api/file-changes/runs/{id}/undo"), json!({})).await;
        if status != 200 {
            return (status, first);
        }
        assert!(["waiting", "writing", "done"].contains(&first["state"].as_str().unwrap_or("")), "{first}");
        let start = Instant::now();
        loop {
            let runs = self.get("/api/file-changes/runs").await;
            // While it undoes, the run is listed live (waiting, writing) beside its kept self; done, only kept.
            let mine: Vec<&Value> = runs.as_array().unwrap().iter().filter(|r| r["id"] == id).collect();
            if !mine.iter().any(|r| r["state"] != "done")
                && let Some(run) = mine.first()
            {
                return (status, (*run).clone());
            }
            assert!(start.elapsed() < Duration::from_secs(120), "waited for {id}'s Undo: {runs}");
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    fn stop(mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

/// Who calls the server: an account by its session, or nobody. The session goes in a `Cookie` header by hand, as the
/// page's cookie jar would send it: this `reqwest` has no `cookies` feature.
struct Caller<'a> {
    server: &'a Server,
    session: Option<String>,
}

impl Caller<'_> {
    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let r = self.server.http.request(method, format!("{}{path}", self.server.base));
        match &self.session {
            Some(cookie) => r.header(reqwest::header::COOKIE, cookie),
            None => r,
        }
    }

    async fn get(&self, path: &str) -> Value {
        let r = self.request(reqwest::Method::GET, path).send().await.unwrap();
        let status = r.status();
        let body = r.text().await.unwrap();
        assert!(status.is_success(), "GET {path}: {status} {body}");
        serde_json::from_str(&body).unwrap()
    }

    /// A call that may be refused: (status, answer).
    async fn send(&self, method: reqwest::Method, path: &str, body: Value) -> (u16, Value) {
        let r = self.request(method, path).json(&body).send().await.unwrap();
        let status = r.status().as_u16();
        (status, r.json().await.unwrap_or(Value::Null))
    }

    async fn ok(&self, method: reqwest::Method, path: &str, body: Value) -> Value {
        let (status, v) = self.send(method.clone(), path, body).await;
        assert!((200..300).contains(&status), "{method} {path}: {status} {v}");
        v
    }

    /// A file as it is served: (status, its bytes), which tell which copy came.
    async fn bytes(&self, path: &str) -> (u16, Vec<u8>) {
        let r = self.request(reqwest::Method::GET, path).send().await.unwrap();
        (r.status().as_u16(), r.bytes().await.unwrap().to_vec())
    }

    /// The albums as the Library lists them to this caller.
    async fn albums(&self) -> Vec<Value> {
        let v = self.get("/api/albums").await;
        v["sources"].as_array().into_iter().flatten().filter(|s| s["id"] == "cratarr").flat_map(|s| s["albums"].as_array().cloned().unwrap_or_default()).collect()
    }
}

/// Settings as 0.15 left them: version 3, the library's roles, `folder` as the Music folder (a server looks for music in
/// its mounts and there, decision 0009).
fn settings_0_15(data: &Path, folder: &str) {
    let settings = json!({
        "version": 3,
        "music_folder": folder,
        "sources": [{ "id": "cratarr", "kind": "cratarr", "enabled": true, "libraries": { "music": [folder] }, "values": { "folders": [folder] } }],
    });
    std::fs::write(data.join("settings.json"), serde_json::to_vec_pretty(&settings).unwrap()).unwrap();
}

/// Settings as 0.16 writes them: version 4, these libraries in order, each (folder, kind), and the own-music folder.
fn settings(data: &Path, libraries: &[(&Path, &str)], own_music: &Path) {
    let folders: Vec<String> = libraries.iter().map(|(folder, _)| folder.to_string_lossy().to_string()).collect();
    let list: Vec<Value> = folders.iter().zip(libraries).map(|(id, (_, kind))| json!({ "id": id, "kind": kind })).collect();
    let settings = json!({
        "version": 4,
        "music_folder": folders[0],
        "sources": [{ "id": "cratarr", "kind": "cratarr", "enabled": true, "libraries": list, "values": { "folders": folders, "own_music": own_music.to_string_lossy() } }],
    });
    std::fs::write(data.join("settings.json"), serde_json::to_vec_pretty(&settings).unwrap()).unwrap();
}

/// A file's values of one Vorbis field, as the file keeps it.
fn vorbis(path: &Path, key: &str) -> Vec<String> {
    use lofty::prelude::*;
    let file = lofty::flac::FlacFile::read_from(&mut std::fs::File::open(path).unwrap(), lofty::config::ParseOptions::new()).unwrap();
    file.vorbis_comments().map(|vc| vc.get_all(key).map(str::to_string).collect()).unwrap_or_default()
}

/// Sets a Vorbis field of a file from outside Cratarr (a person's tagger).
fn set_vorbis(path: &Path, key: &str, value: &str) {
    use lofty::prelude::*;
    let mut file = lofty::flac::FlacFile::read_from(&mut std::fs::File::open(path).unwrap(), lofty::config::ParseOptions::new()).unwrap();
    let mut vc = file.remove_vorbis_comments().unwrap_or_default();
    vc.remove(key).for_each(drop);
    vc.push(key.into(), value.into());
    file.set_vorbis_comments(vc);
    file.save_to_path(path, lofty::config::WriteOptions::default()).unwrap();
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cratarr-e2e-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Every audio file under `dir`, relative to it, sorted.
fn audio_under(dir: &Path) -> Vec<String> {
    fn walk(root: &Path, at: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(at).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else if p.extension().is_some_and(|x| x == "flac" || x == "mp3") {
                out.push(p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// The 0.16 first start's migration of 0.15's playlists (decision 0101 points 12 and 28), rehearsed on the fixture
/// library: the `.m3u` files of Music's playlist folder and an account's folder there, one line naming a file that is
/// gone, and 0.15's user data beside them (shared, seen, an old name). Brought into the database once, by their new ids,
/// their copies written out, the gone line kept missing, the report saying so; a restart brings nothing in again.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn the_playlists_of_0_15_come_into_the_database_end_to_end() {
    let root = temp("playlists");
    let (music, data) = (root.join("music"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    assert!(Command::new(BIN).arg("--make-fixtures").arg(&music).status().unwrap().success(), "the fixture library is made");
    let files = audio_under(&music);
    let folder = music.to_string_lossy().to_string();
    let lists = music.join(".playlists");
    std::fs::create_dir_all(lists.join("ann")).unwrap();
    std::fs::write(lists.join("Mix.m3u"), format!("#EXTM3U\n../{}\n../{}\n../Gone/01 Gone.flac\n", files[0], files[1])).unwrap();
    std::fs::write(lists.join("ann").join("Evening.m3u"), format!("../../{}\n", files[2])).unwrap();
    {
        let ud = rusqlite::Connection::open(data.join("userdata.sqlite")).unwrap();
        ud.execute_batch(
            "CREATE TABLE playlist_shared (playlist TEXT PRIMARY KEY, at INTEGER NOT NULL);
             INSERT INTO playlist_shared VALUES ('ann/Evening', 1);
             CREATE TABLE playlist_alias (old TEXT PRIMARY KEY, name TEXT NOT NULL, at INTEGER NOT NULL);
             INSERT INTO playlist_alias VALUES ('Old Mix', 'Mix', 1);
             CREATE TABLE playlist_seen (name TEXT PRIMARY KEY, print TEXT NOT NULL, created INTEGER NOT NULL, changed INTEGER NOT NULL);
             INSERT INTO playlist_seen VALUES ('Mix', '', 100, 200);",
        )
        .unwrap();
    }
    settings_0_15(&data, &folder);

    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.until("the fixture library read", Duration::from_secs(120), || async { s.albums().await.len() == 12 }).await;

    // Brought in by their new ids, another's shared one too, as it was.
    let view = s.get("/api/state").await;
    let list = view["playlists"].as_array().cloned().unwrap_or_default();
    let mix = list.iter().find(|p| p["name"] == "Mix").cloned().expect("Mix brought in");
    let evening = list.iter().find(|p| p["name"] == "Evening").cloned().expect("ann's shared Evening brought in");
    let id = mix["id"].as_str().unwrap().to_string();
    assert!(id.starts_with("pl-") && id.len() == 15, "its own id: {id}");
    assert_eq!((evening["owner"].as_str(), evening["shared"].as_bool(), evening["mine"].as_bool()), (Some("ann"), Some(true), Some(false)));
    // 0.15's keys lead to the ids, the old name too.
    let moved = s.get("/api/playlists/moved").await;
    assert_eq!((moved["Mix"].as_str(), moved["Old Mix"].as_str()), (Some(id.as_str()), Some(id.as_str())));
    assert_eq!(moved["ann/Evening"], evening["id"]);
    // The gone line kept, missing; the others its tracks.
    let rows = s.get(&format!("/api/playlists/{id}/rows")).await;
    let rows = rows["rows"].as_array().cloned().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows.iter().map(|r| r["missing"].as_bool().unwrap()).collect::<Vec<_>>(), [false, false, true]);
    // The copies, beside the database, never read back.
    assert_eq!(std::fs::read_to_string(data.join("playlists").join("Mix.m3u")).unwrap(), format!("{}\n{}\nGone/01 Gone.flac\n", files[0], files[1]));
    assert!(data.join("playlists").join("ann").join("Evening.m3u").is_file());
    // The report: what came, what found no match.
    let report = s.get("/api/admin/migration").await;
    let part = &report["parts"]["playlists"];
    assert_eq!((part["playlists"].as_u64(), part["lines"].as_u64()), (Some(2), Some(4)), "{part}");
    let at = part["at"].as_i64().unwrap();
    // The libraries' roles became the list (0101 point 1), the file 0.16's, every library listed by its server.
    let libraries = &report["parts"]["libraries"];
    assert_eq!((libraries["tables"][0]["before"].as_u64(), libraries["tables"][2]["after"].as_u64()), (Some(1), Some(1)), "{libraries}");
    assert_eq!(libraries["unmatched"][1]["count"].as_u64(), Some(0), "{libraries}");
    let file: serde_json::Value = serde_json::from_slice(&std::fs::read(data.join("settings.json")).unwrap()).unwrap();
    assert_eq!((file["version"].as_u64(), file["sources"][0]["libraries"].clone()), (Some(4), json!([{ "id": folder, "kind": "main" }])));
    s.stop();

    // Started again: nothing is brought in twice.
    let s = Server::start(&data).await;
    let again = s.get("/api/admin/migration").await;
    assert_eq!(again["parts"]["playlists"]["at"].as_i64(), Some(at), "once");
    let view = s.get("/api/state").await;
    let list = view["playlists"].as_array().cloned().unwrap_or_default();
    assert_eq!(list.iter().filter(|p| p["name"] == "Mix").count(), 1);
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// The harness for who hears what (decision 0101), on every kind of library and settings as 0.16 writes them: two
/// accounts signed in by their sessions. Each is itself to the server, nobody is refused, and a file comes as it lies.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn two_accounts_sign_in_on_every_kind_of_library_end_to_end() {
    let root = temp("accounts");
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    assert!(Command::new(BIN).arg("--make-layout").arg(&library).status().unwrap().success(), "the whole layout is made");
    let own = library.join("own");
    settings(&data, &[(&library.join("music"), "main"), (&library.join("second"), "main"), (&library.join("incoming"), "incoming")], &own);

    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    // A server with no accounts takes any caller for its administrator: so the first account comes to be.
    s.ok(reqwest::Method::POST, "/api/users", json!({ "name": "ann", "password": "ann's password", "admin": true })).await;
    assert_eq!(s.send(reqwest::Method::GET, "/api/albums", json!({})).await.0, 401, "no session, no answer");
    let ann = s.sign_in("ann", "ann's password").await;
    ann.ok(reqwest::Method::POST, "/api/users", json!({ "name": "bob", "password": "bob's password" })).await;
    let bob = s.sign_in("bob", "bob's password").await;
    assert_eq!(bob.send(reqwest::Method::POST, "/api/users", json!({ "name": "cid", "password": "x" })).await.0, 403, "bob is no administrator");

    // Every library read: the albums' titles, as these stay however the copies of a record are listed.
    let titles = |albums: Vec<Value>| albums.iter().filter_map(|a| a["title"].as_str().map(str::to_string)).collect::<std::collections::BTreeSet<String>>();
    s.until("every library read", Duration::from_secs(180), || async { titles(ann.albums().await).len() == 22 }).await;
    let shown = titles(bob.albums().await);
    assert!(["Tide Tables", "Far Bank", "Just Arrived", "Own Room", "Other Room"].iter().all(|t| shown.contains(*t)), "{shown:?}");

    // Each is who they signed in as: whose own music is whose.
    let own_music = |view: Value| -> Vec<(String, bool)> {
        let libraries = view["sources"].as_array().into_iter().flatten().flat_map(|s| s["library_info"].as_array().cloned().unwrap_or_default());
        libraries.filter(|l| l["kind"] == "own").map(|l| (l["owner"].as_str().unwrap_or("").to_string(), l["mine"].as_bool().unwrap_or(false))).collect()
    };
    assert_eq!(own_music(ann.get("/api/albums").await), [("ann".to_string(), true), ("bob".to_string(), false)]);
    assert_eq!(own_music(bob.get("/api/albums").await), [("ann".to_string(), false), ("bob".to_string(), true)]);

    // A file comes as it lies on the disk, and to a session only.
    let room = ann.albums().await.into_iter().find(|a| a["title"] == "Own Room").unwrap();
    let album = ann.get(&format!("/api/albums/cratarr/{}", room["id"].as_str().unwrap())).await;
    let file = format!("/api/file/cratarr/{}", album["rows"][0]["id"].as_str().unwrap());
    let (status, bytes) = ann.bytes(&file).await;
    assert_eq!(status, 200);
    assert!(bytes == std::fs::read(own.join("ann/Salt Meridian/Own Room (2018)/01 Own Room.mp3")).unwrap(), "not the file as it lies");
    assert_eq!(s.anyone().bytes(&file).await.0, 401);
    s.stop();

    // A server that asks for a sign-in starts as any other.
    let s = Server::start(&data).await;
    assert_eq!(s.sign_in("bob", "bob's password").await.get("/api/albums").await["sources"][0]["id"], "cratarr");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn types_and_years_into_the_files_end_to_end() {
    let root = temp("types");
    let (music, data) = (root.join("music"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    let made = Command::new(BIN).arg("--make-fixtures").arg(&music).status().unwrap();
    assert!(made.success(), "the fixture library is made");
    settings_0_15(&data, &music.to_string_lossy());

    let s = Server::start(&data).await;
    // Nothing from outside: MusicBrainz off before anything asks it.
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.until("the fixture library read", Duration::from_secs(120), || async { s.albums().await.len() == 12 }).await;

    // Off: nothing offered, nothing applied.
    let (status, _) = s.send(reqwest::Method::POST, "/api/file-changes/plans/types/apply", json!({ "albums": ["x"] })).await;
    assert_eq!(status, 409, "refused while switched off");
    let on = s.ok(reqwest::Method::PUT, "/api/settings/file-facts", json!({ "on": true })).await;
    assert!(on["differ"].as_u64().unwrap() > 0, "records differ: {on}");

    // The sheet: the undated album's year is unknown, so it is not offered a date; the others are offered types.
    let sheet = s.get("/api/file-changes/plans/types").await;
    let records = sheet["records"].as_array().unwrap().clone();
    assert!(!records.is_empty());
    let tide = records.iter().find(|r| r["title"] == "Tide Tables").expect("Tide Tables offered");
    let tide_album = tide["album"].as_str().unwrap().to_string();
    assert!(tide["fields"].as_array().unwrap().iter().any(|f| f["field"] == "releasetype" && f["track"].is_null()), "said once for every file: {tide}");

    let tide_stamp = tide["stamp"].as_str().expect("a sheet's record has its stamp").to_string();
    let tide_dir = music.join("Glass Harbour/Tide Tables (2012)");
    let (first, second) = (tide_dir.join("01 Slack Water.flac"), tide_dir.join("02 Neap.flac"));
    let style = vorbis(&second, "STYLE");
    assert_eq!(style, ["Dub Techno", "Ambient"]);

    // Changed since it was planned: a tagger changes a file between the sheet and Write. However soon the catalogue
    // reads it, Cratarr never writes over it: the record (its stamp) or the file (read again first) is set aside.
    set_vorbis(&first, "RELEASETYPE", "single");
    let started = s.ok(reqwest::Method::POST, "/api/file-changes/plans/types/apply", json!({ "albums": [tide_album], "stamps": { tide_album.clone(): tide_stamp } })).await;
    assert!(["waiting", "writing", "done"].contains(&started["state"].as_str().unwrap()), "{started}");
    let a = s.done_run(None).await;
    let why: Vec<&str> = a["failed"].as_array().unwrap().iter().map(|f| f["why"].as_str().unwrap()).collect();
    assert!(!why.is_empty() && why.iter().all(|w| w.contains("changed since it was planned")), "{a}");
    assert_eq!(vorbis(&first, "RELEASETYPE"), ["single"], "the tagger's change kept");
    let a_id = a["id"].as_str().unwrap().to_string();

    // A fresh sheet shows the tagger's file as it is now; Write: every file of the record gets Cratarr's type (six
    // tracks of a few seconds: an EP by the rule), the fields lofty has no key for kept.
    let sheet = s.get(&format!("/api/file-changes/plans/types?albums={tide_album}")).await;
    let record = &sheet["records"][0];
    s.ok(reqwest::Method::POST, "/api/file-changes/plans/types/apply", json!({ "albums": [tide_album], "stamps": { tide_album.clone(): record["stamp"] } })).await;
    let b = s.done_run(Some(&a_id)).await;
    assert!(b["failed"].as_array().unwrap().is_empty(), "{b}");
    for f in std::fs::read_dir(&tide_dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "flac")) {
        assert_eq!(vorbis(&f, "RELEASETYPE"), ["ep"], "{}", f.display());
        assert_eq!(vorbis(&f, "MUSICBRAINZ_ALBUMTYPE"), ["ep"], "Lidarr's second field follows: {}", f.display());
    }
    assert_eq!(vorbis(&second, "STYLE"), style, "a field lofty has no key for, kept");
    assert_eq!(vorbis(&second, "ORIGINALYEAR"), ["2012"], "a key's second spelling, kept apart");
    assert_eq!(vorbis(&second, "ORIGINALDATE"), ["2012-03-05"]);
    let b_id = b["id"].as_str().unwrap().to_string();

    // Kept across a restart.
    let detail = s.get(&format!("/api/file-changes/runs/{b_id}")).await;
    assert_eq!(detail["records"][0]["album"], tide_album);
    s.stop();
    let s = Server::start(&data).await;
    let runs = s.get("/api/file-changes/runs").await;
    for id in [&a_id, &b_id] {
        assert!(runs.as_array().unwrap().iter().any(|r| r["id"] == id.as_str()), "{id} kept across a restart: {runs}");
    }

    // Undo the second run, then the first: every file as it was before Cratarr, the tagger's change too.
    let (_, undone) = s.undo(&b_id).await;
    assert_eq!(undone["undone"], true, "{undone}");
    assert_eq!(vorbis(&first, "RELEASETYPE"), ["single"]);
    let (status, _) = s.send(reqwest::Method::POST, &format!("/api/file-changes/runs/{b_id}/undo"), json!({})).await;
    assert_eq!(status, 409, "undone once");
    // The first run wrote nothing where its record was set aside whole; else its files go back too.
    let (status, _) = s.undo(&a_id).await;
    assert!(status == 200 || status == 409, "{status}");
    assert_eq!(vorbis(&second, "RELEASETYPE"), ["album"], "back as it was");
    assert_eq!(vorbis(&second, "MUSICBRAINZ_ALBUMTYPE"), ["album"]);
    assert_eq!(vorbis(&first, "RELEASETYPE"), ["single"]);
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}
