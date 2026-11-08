//! The end-to-end suite (decision 0093): the real server binary on a fresh copy of the fixture library and a fresh data
//! folder, driven through its API as the page drives it. Types into the files (decision 0092): the catalogue read, the
//! review sheet, a run written and read back from the files, "changed since it was planned", Undo, and the run kept
//! across a restart. Everything is thrown away after. The Dockerfile's test and `e2e` stages run them by name, so a test
//! here ends its name in `end_to_end` or the gate never runs it: `cargo test --workspace -- --ignored fixtures:: end_to_end`.
//! What the records' rewrite must not break has a test of its own, each with its Undo: Keep, Delete, Replace, a length's fix
//! and a cover, and a second run waiting for the one writing. Last, a library changing place, for everything a listener holds.
//!
//! `Server` calls as nobody, which a server with no accounts takes for its administrator. `Server::sign_in` gives a
//! `Caller` that is an account, `settings` writes 0.16's settings for the whole layout (`--make-layout`), `settings_0_15`
//! what 0.15 left.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, OnceLock};
use std::sync::atomic::{AtomicUsize, Ordering};
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
        Self::start_with(data, &[]).await
    }

    /// `start`, with these in its environment.
    async fn start_with(data: &Path, env: &[(&str, &str)]) -> Server {
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let child = Command::new(BIN)
            .env("CRATARR_DATA_DIR", data)
            .env("CRATARR_BIND", format!("127.0.0.1:{port}"))
            .env("RUST_LOG", "warn")
            .envs(env.iter().copied())
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

    /// Every record the types sheet offers, written: told to go, not gone.
    async fn write_the_sheet(&self) {
        self.write_sheet("types").await
    }

    /// Every record a kind's sheet offers, written: told to go, not gone.
    async fn write_sheet(&self, kind: &str) {
        let sheet = self.get(&format!("/api/file-changes/plans/{kind}")).await;
        let records = sheet["records"].as_array().unwrap();
        assert!(!records.is_empty(), "something to write");
        let albums: Vec<&Value> = records.iter().map(|r| &r["album"]).collect();
        let stamps: serde_json::Map<String, Value> = records.iter().map(|r| (r["album"].as_str().unwrap().to_string(), r["stamp"].clone())).collect();
        self.ok(reqwest::Method::POST, &format!("/api/file-changes/plans/{kind}/apply"), json!({ "albums": albums, "stamps": stamps })).await;
    }

    /// An album with a copy in `library` as the Library lists it, if it does. A record is one entry, and its `library` is
    /// only the copy that plays.
    async fn album_in(&self, title: &str, library: &Path) -> Option<Value> {
        let library = library.to_string_lossy();
        let holds = |a: &Value| a["copies"].as_array().is_some_and(|c| c.iter().any(|c| c["library"].as_str() == Some(&*library)));
        self.albums().await.into_iter().find(|a| a["title"] == title && holds(a))
    }

    /// That album once the library has read it there.
    async fn read_in(&self, title: &str, library: &Path) -> Value {
        let start = Instant::now();
        loop {
            if let Some(album) = self.album_in(title, library).await {
                return album;
            }
            assert!(start.elapsed() < Duration::from_secs(120), "waited for {title} in {}", library.display());
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    /// An album Incoming lists: read again after a build or at the next pass, not as a file comes.
    async fn incoming_album(&self, title: &str) -> Value {
        let start = Instant::now();
        loop {
            let view = self.get("/api/incoming").await;
            let mut albums = view["sources"].as_array().into_iter().flatten().flat_map(|s| s["albums"].as_array().cloned().unwrap_or_default());
            if let Some(album) = albums.find(|a| a["title"] == title) {
                return album;
            }
            assert!(start.elapsed() < Duration::from_secs(120), "waited for {title} in Incoming: {view}");
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    /// An album's songs, whose ids are its tracks': what must survive Keep and Replace.
    async fn songs_of(&self, album: &Value) -> Vec<String> {
        let album = self.get(&format!("/api/albums/cratarr/{}", album["id"].as_str().unwrap())).await;
        album["rows"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap().to_string()).collect()
    }

    /// An album action on the list of runs, by the journal's id (`m…`): its files have moved.
    async fn album_run(&self, what: &str) -> Value {
        let runs = self.get("/api/file-changes/runs").await;
        let found = runs.as_array().unwrap().iter().find(|r| r["what"] == what && r["id"].as_str().unwrap().starts_with('m'));
        found.unwrap_or_else(|| panic!("no {what} on the list: {runs}")).clone()
    }

    /// An album action's Undo as a person gives it, a moment after the action: within the second of the library's last look,
    /// the catalogue's change goes unseen until the next pass (`an_undo_in_the_second_of_its_keep_is_seen_before_the_next_pass`).
    async fn undo_a_moment_later(&self, id: &str) -> (u16, Value) {
        tokio::time::sleep(Duration::from_secs(2)).await;
        self.undo(id).await
    }

    /// A file as it is served for a song.
    async fn served(&self, song: &str) -> Vec<u8> {
        let (status, bytes) = self.anyone().bytes(&format!("/api/file/cratarr/{song}")).await;
        assert_eq!(status, 200, "{song}");
        bytes
    }

    /// A picture for an album, as the page uploads it.
    async fn upload_cover(&self, album: &Value, picture: &[u8]) -> Value {
        let url = format!("{}/api/albums/cratarr/{}/covers", self.base, album["id"].as_str().unwrap());
        let sent = self.http.put(url).body(picture.to_vec()).send().await.unwrap();
        assert!(sent.status().is_success(), "{}", sent.status());
        sent.json().await.unwrap()
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

/// A file a list shows, as a catalogue says it.
const PRESENT: &str = "folder >= 0 AND gone_at IS NULL AND trashed IS NULL";

/// A data folder as 0.15 left it over `--make-layout`'s files (`tests/fixtures/0.15`, written down from tag v0.15.0, as 0093
/// keeps no binaries in git): its catalogue, so its track ids mean what its user data says they mean; its user data, its journal
/// and its sync under 0.15's ids; each account's crates, queues, jobs, playlists and settings. 0.15 had one Music folder, so the
/// layout's second library is an Incoming there, and own music did not exist. The folders and each file's size and time are this
/// run's: as they stand in the text, every file would read as changed.
fn data_0_15(library: &Path, data: &Path) {
    let here = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/0.15");
    let (music, incoming, second) = (library.join("music"), library.join("incoming"), library.join("second"));
    let folders = [music.as_path(), incoming.as_path(), second.as_path()];
    let text = |name: &str| {
        let sql = std::fs::read_to_string(here.join(name)).unwrap();
        let places = ["@music@", "@incoming@", "@second@"].into_iter().zip(folders);
        format!("BEGIN;\n{}\nCOMMIT;", places.fold(sql, |sql, (name, folder)| sql.replace(name, &folder.to_string_lossy())))
    };
    let db = rusqlite::Connection::open(data.join("catalogue.sqlite")).unwrap();
    db.execute_batch(&text("catalogue.sql")).unwrap();
    // What no list shows has no file here, but a file in a trash has its place there, or the next pass lets it go.
    let token: String = db.query_row("SELECT trashed FROM files WHERE trashed IS NOT NULL", [], |r| r.get(0)).unwrap();
    std::fs::create_dir_all(incoming.join(".cratarr-trash").join(token)).unwrap();
    for (table, here) in [("files", PRESENT), ("cover_files", "1")] {
        let rows: Vec<(i64, String)> = db.prepare(&format!("SELECT folder, rel FROM {table} WHERE {here}")).unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
        for (folder, rel) in rows {
            let meta = std::fs::metadata(folders[folder as usize].join(&rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
            let mtime = meta.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
            db.execute(&format!("UPDATE {table} SET size = ?1, mtime = ?2 WHERE folder = ?3 AND rel = ?4"), rusqlite::params![meta.len() as i64, mtime, folder, rel]).unwrap();
        }
    }
    drop(db);
    for (file, sql) in [("userdata.sqlite", "userdata.sql"), ("journal.sqlite", "journal.sql"), ("playlist-sync.sqlite", "playlist-sync.sql")] {
        rusqlite::Connection::open(data.join(file)).unwrap().execute_batch(&text(sql)).unwrap();
    }
    std::fs::create_dir_all(data.join("users/bob")).unwrap();
    for file in ["crates.json", "users/bob/crates.json", "subsonic-users.json", "queues.json", "convert-queue.json", "track-intake.json"] {
        std::fs::copy(here.join(file), data.join(file)).unwrap();
    }
    // A Sounds like's picture goes by its crate's id.
    std::fs::create_dir_all(data.join("crate-images")).unwrap();
    let like: String = "like:al-4Jx9dQszNKgnbo4mZtic7y".bytes().map(|b| format!("{b:02x}")).collect();
    std::fs::write(data.join("crate-images").join(format!("x{like}.jpg")), b"a picture").unwrap();
    std::fs::create_dir_all(music.join(".playlists/ann")).unwrap();
    for list in ["Mix.m3u", "ann/Evening.m3u"] {
        std::fs::copy(here.join("playlists").join(list), music.join(".playlists").join(list)).unwrap();
    }
    let (music, incoming, second) = (music.to_string_lossy(), incoming.to_string_lossy(), second.to_string_lossy());
    let settings = json!({
        "version": 3,
        "music_folder": music,
        "sources": [{ "id": "cratarr", "kind": "cratarr", "enabled": true, "libraries": { "music": [music], "incoming": [incoming, second] }, "values": { "folders": [music, incoming, second] } }],
    });
    std::fs::write(data.join("settings.json"), serde_json::to_vec_pretty(&settings).unwrap()).unwrap();
}

/// Cratarr's own source as 0.16 keeps it and the page sends it: these libraries in order, each (folder, kind), and the own-music
/// folder.
fn own_source(libraries: &[(&Path, &str)], own_music: &Path) -> Value {
    let folders: Vec<String> = libraries.iter().map(|(folder, _)| folder.to_string_lossy().to_string()).collect();
    let list: Vec<Value> = folders.iter().zip(libraries).map(|(id, (_, kind))| json!({ "id": id, "kind": kind })).collect();
    json!({ "id": "cratarr", "kind": "cratarr", "enabled": true, "libraries": list, "values": { "folders": folders, "own_music": own_music.to_string_lossy() } })
}

/// Settings as 0.16 writes them: version 4, these libraries in order, each (folder, kind), and the own-music folder.
fn settings(data: &Path, libraries: &[(&Path, &str)], own_music: &Path) {
    let settings = json!({ "version": 4, "music_folder": libraries[0].0.to_string_lossy(), "sources": [own_source(libraries, own_music)] });
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

/// What `flag` makes, made once a run and copied to `to`, the test's own to change. Writing and encoding it takes a debug
/// build most of a minute, and each test paid that.
fn made_once(once: &'static OnceLock<PathBuf>, flag: &str, to: &Path) {
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap().flatten() {
            let (p, q) = (e.path(), to.join(e.file_name()));
            if p.is_dir() {
                copy(&p, &q);
            } else {
                std::fs::copy(&p, &q).unwrap();
            }
        }
    }
    let made = once.get_or_init(|| {
        let at = temp(flag.trim_start_matches('-')).join("made");
        assert!(Command::new(BIN).arg(flag).arg(&at).status().unwrap().success(), "{flag} made nothing");
        at
    });
    std::fs::remove_dir_all(to).ok();
    copy(made, to);
}

/// The whole layout at `to`.
fn make_layout(to: &Path) {
    static MADE: OnceLock<PathBuf> = OnceLock::new();
    made_once(&MADE, "--make-layout", to);
}

/// The fixture library at `to`.
fn make_fixtures(to: &Path) {
    static MADE: OnceLock<PathBuf> = OnceLock::new();
    made_once(&MADE, "--make-fixtures", to);
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
    make_fixtures(&music);
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

/// A crate of the fixture's Undated, four tracks of one genre, on a server of its own: (server, root, the crate, its tracks).
async fn a_crate_of_four(name: &str) -> (Server, PathBuf, String, Vec<String>) {
    let root = temp(name);
    let (music, data) = (root.join("music"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_fixtures(&music);
    settings_0_15(&data, &music.to_string_lossy());
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.until("the fixture library read", Duration::from_secs(120), || async { s.albums().await.len() == 12 }).await;
    let genre = json!([{ "axis": "genre", "value": { "kind": "any_of", "values": ["Downtempo"] } }]);
    let made = s.ok(reqwest::Method::POST, "/api/crates", json!({ "name": "Downtempo", "conditions": genre })).await;
    let id = made["id"].as_str().unwrap().to_string();
    let tracks = row_ids(&s.get(&format!("/api/crates/{id}/rows")).await);
    assert_eq!(tracks.len(), 4);
    (s, root, id, tracks)
}

/// A list's rows by their ids.
fn row_ids(list: &Value) -> Vec<String> {
    list["rows"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap().to_string()).collect()
}

/// A crate's tracks left out are tracks (0101: lines to tracks) at every door: its rows, the list that puts them back, the
/// store, Undo, and what a Sounds like learns from what was turned down for it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_track_left_out_is_the_track_at_every_door_end_to_end() {
    let (s, root, id, tracks) = a_crate_of_four("left-out").await;
    let (rows, left) = (format!("/api/crates/{id}/rows"), format!("/api/crates/{id}/left-out"));
    let out = s.ok(reqwest::Method::POST, &left, json!({ "ids": [tracks[0]] })).await;
    assert_eq!(out["left_out"], 1);
    assert_eq!(row_ids(&s.get(&rows).await), tracks[1..], "out of the crate");
    assert_eq!(row_ids(&s.get(&left).await), tracks[..1], "on the list that puts it back");
    let store: Value = serde_json::from_slice(&std::fs::read(root.join("data/crates.json")).unwrap()).unwrap();
    assert_eq!(store["left_out"][&id], json!([tracks[0]]), "by its track");
    let undone = s.ok(reqwest::Method::POST, "/api/undo", json!({ "tokens": [out["undo"]] })).await;
    assert_eq!(row_ids(&s.get(&rows).await), tracks, "undone: {undone}");
    s.ok(reqwest::Method::POST, &left, json!({ "ids": [tracks[0]] })).await;
    let back = s.ok(reqwest::Method::DELETE, &left, json!({ "ids": [tracks[0]] })).await;
    assert_eq!(back["left_out"], 0);
    assert_eq!(row_ids(&s.get(&rows).await), tracks, "put back");

    // Turned down for a Sounds like: what it says of what does not fit reads the same tracks.
    let like = format!("like:{}", tracks[1]);
    s.ok(reqwest::Method::POST, &format!("/api/crates/{like}/left-out"), json!({ "ids": [tracks[2]] })).await;
    assert_eq!(s.get(&format!("/api/crates/{like}/fit")).await["turned_down"], 1);
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// What is excluded everywhere is a track at every door: the list, every crate, Sounds like's curve, the store.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_track_excluded_is_the_track_at_every_door_end_to_end() {
    let (s, root, id, tracks) = a_crate_of_four("excluded").await;
    let rows = format!("/api/crates/{id}/rows");
    let curve = format!("/api/like-curve?of={}", tracks[1]);
    let alike = s.get(&curve).await["alike"].as_array().unwrap().len();
    assert!(alike > 1, "a curve to tell by");
    let out = s.ok(reqwest::Method::POST, "/api/excluded", json!({ "ids": [tracks[0]] })).await;
    assert_eq!(out["excluded"], 1);
    assert_eq!(row_ids(&s.get("/api/excluded").await), tracks[..1], "on the list that puts it back");
    assert_eq!(row_ids(&s.get(&rows).await), tracks[1..], "out of every crate");
    let store: Value = serde_json::from_slice(&std::fs::read(root.join("data/crates.json")).unwrap()).unwrap();
    assert_eq!(store["excluded"], json!([tracks[0]]), "by its track");
    assert_eq!(s.get(&curve).await["alike"].as_array().unwrap().len(), alike - 1, "out of what is like anything");
    let back = s.ok(reqwest::Method::DELETE, "/api/excluded", json!({ "ids": [tracks[0]] })).await;
    assert_eq!(back["excluded"], 0);
    assert_eq!(row_ids(&s.get(&rows).await), tracks, "put back");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// A record deleted with a track of it left out, its crate's list read and the track gone from the library.
async fn left_out_and_deleted(name: &str) -> (Server, PathBuf, String, String) {
    let (s, root, id, tracks) = a_crate_of_four(name).await;
    let left = format!("/api/crates/{id}/left-out");
    s.ok(reqwest::Method::POST, &left, json!({ "ids": [tracks[0]] })).await;
    let album = s.albums().await.into_iter().find(|a| a["title"] == "Undated").unwrap();
    s.ok(reqwest::Method::POST, &format!("/api/albums/cratarr/{}/delete", album["id"].as_str().unwrap()), json!({})).await;
    s.until("the library to let it go", Duration::from_secs(120), || async { !s.albums().await.iter().any(|a| a["title"] == "Undated") }).await;
    (s, root, left, tracks[0].clone())
}

/// The list shows what a track was once its file went, by the id it keeps: a line would say nothing but a path.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_track_left_out_shows_what_it_was_after_its_file_went_end_to_end() {
    let (s, root, left, track) = left_out_and_deleted("left-out-shown").await;
    let list = s.get(&left).await;
    assert_eq!(row_ids(&list), [track], "{list}");
    assert_eq!(list["rows"][0]["name"], "Morning", "{list}");
    assert_eq!(list["rows"][0]["artist"], "Kite Theory", "{list}");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// What was left out can be put back after its file went: taken in, a track must be there, but going out it need not be.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_track_left_out_can_be_put_back_after_its_file_went_end_to_end() {
    let (s, root, left, track) = left_out_and_deleted("left-out-back").await;
    assert_eq!(s.ok(reqwest::Method::DELETE, &left, json!({ "ids": [track] })).await["left_out"], 0);
    assert_eq!(s.ok(reqwest::Method::POST, &left, json!({ "ids": [track] })).await["left_out"], 0, "not there, so not taken in");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

const FAR: &str = "Salt Meridian/Far Bank (2018)";
const THREE: &str = "Salt Meridian/Three Banks (2018)";

/// Two real files, two tracks for now, each with an id anyone can name: Far Bank, alone on its record in Music, and a track of that
/// name and length on an album of three in the second library. `take` makes the second a copy of the first.
struct Banks {
    data: PathBuf,
    second: PathBuf,
    stays: String,
    goes: String,
    others: Vec<String>,
}

impl Banks {
    /// A person moves the second file out of its album and tags it as Far Bank. Its album leaves its record to the two left, so it
    /// is held against the others and is a copy of Far Bank, and its track is taken at the next build. A Delete of something else
    /// asks for a pass, which a file moved outside Cratarr waits for. Back once what names the track has followed it.
    async fn take(&self, who: &Caller<'_>) {
        let (from, to) = (self.second.join(THREE).join("01 Far Bank.flac"), self.second.join(FAR));
        set_vorbis(&from, "ALBUM", "Far Bank");
        std::fs::create_dir_all(&to).unwrap();
        std::fs::rename(&from, to.join("01 Far Bank.flac")).unwrap();
        let library = self.second.to_string_lossy();
        let there = |a: &Value| a["copies"].as_array().is_some_and(|c| c.iter().any(|c| c["library"].as_str() == Some(&*library)));
        let other = who.albums().await.into_iter().find(|a| a["title"] == "Two Pressings" && there(a)).unwrap();
        who.ok(reqwest::Method::POST, &format!("/api/albums/cratarr/{}/delete", other["id"].as_str().unwrap()), json!({})).await;
        let s = who.server;
        s.until("its id to go", Duration::from_secs(120), || async { tracks_at(&self.data, &self.second, FAR) == [self.stays.clone()] }).await;
        s.until("what names it to follow", Duration::from_secs(60), || async { self.left_to_follow() == 0 }).await;
    }

    /// How many taken tracks wait for what names them to follow.
    fn left_to_follow(&self) -> i64 {
        let db = rusqlite::Connection::open(self.data.join("catalogue.sqlite")).unwrap();
        db.busy_timeout(Duration::from_secs(10)).unwrap();
        db.query_row("SELECT count(*) FROM track_moved WHERE followed = 0", [], |r| r.get(0)).unwrap()
    }

    /// A crate of the four tracks: the files' own genre.
    async fn crate_of(&self, who: &Caller<'_>) -> String {
        let genre = json!([{ "axis": "genre", "value": { "kind": "any_of", "values": ["Riverside"] } }]);
        let made = who.ok(reqwest::Method::POST, "/api/crates", json!({ "name": "Riverside", "conditions": genre })).await;
        let id = made["id"].as_str().unwrap().to_string();
        assert_eq!(in_order(row_ids(&who.get(&format!("/api/crates/{id}/rows")).await)), in_order([vec![self.stays.clone(), self.goes.clone()], self.others.clone()].concat()));
        id
    }
}

fn in_order(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids
}

async fn two_banks(name: &str) -> (Server, PathBuf, Banks) {
    let root = temp(name);
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_layout(&library);
    let (music, second) = (library.join("music"), library.join("second"));
    std::fs::rename(second.join(FAR), music.join(FAR)).unwrap();
    let far = music.join(FAR).join("01 Far Bank.flac");
    set_vorbis(&far, "GENRE", "Riverside");
    std::fs::create_dir_all(second.join(THREE)).unwrap();
    for (n, title) in ["Far Bank", "Near Bank", "Mid Bank"].into_iter().enumerate() {
        let file = second.join(THREE).join(format!("0{} {title}.flac", n + 1));
        std::fs::copy(&far, &file).unwrap();
        set_vorbis(&file, "ALBUM", "Three Banks");
        set_vorbis(&file, "TITLE", title);
        set_vorbis(&file, "TRACKNUMBER", &(n + 1).to_string());
    }
    settings(&data, &[(&music, "main"), (&second, "main")], &root.join("no-own-music"));
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.read_in("Far Bank", &music).await;
    s.read_in("Three Banks", &second).await;
    let (stays, mut others) = (tracks_at(&data, &music, FAR).remove(0), tracks_at(&data, &second, THREE));
    let goes = others.remove(0);
    assert_ne!(goes, stays, "two tracks for now");
    (s, root, Banks { data, second, stays, goes, others })
}

/// A track left out before its id is taken (CR6: the list names the track, so nothing hides a take from it) stays out, read from
/// either file. A stop before the mark goes over it again and writes nothing, and an Undo from before the take finds the list as it
/// left it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_track_left_out_stays_out_when_its_id_is_taken_end_to_end() {
    let (s, root, banks) = two_banks("taken-left-out").await;
    let id = banks.crate_of(&s.anyone()).await;
    let (rows, left) = (format!("/api/crates/{id}/rows"), format!("/api/crates/{id}/left-out"));
    let out = s.ok(reqwest::Method::POST, &left, json!({ "ids": [banks.goes] })).await;
    assert_eq!(row_ids(&s.get(&rows).await).len(), 3, "the other file is another track for now");

    banks.take(&s.anyone()).await;
    assert_eq!(in_order(row_ids(&s.get(&rows).await)), in_order(banks.others.clone()), "the track stays out, whichever file it is read from");
    assert_eq!(row_ids(&s.get(&left).await), [banks.stays.clone()], "on the list that puts it back");
    let (file, before) = (root.join("data/crates.json"), root.join("data/crates.json.bak"));
    let store: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(store["left_out"][&id], json!([banks.stays]), "by the track that took it");

    // Stopped before the mark: `refresh` goes over it at its head, and a route that sets a fact asks for one at once.
    let (kept, previous) = (std::fs::read(&file).unwrap(), std::fs::read(&before).unwrap());
    let db = rusqlite::Connection::open(banks.data.join("catalogue.sqlite")).unwrap();
    db.busy_timeout(Duration::from_secs(10)).unwrap();
    db.execute("UPDATE track_moved SET followed = 0", []).unwrap();
    assert!(banks.left_to_follow() > 0);
    let album = s.albums().await.into_iter().find(|a| a["title"] == "Both Shores").unwrap();
    s.ok(reqwest::Method::PUT, "/api/albums/overrides", json!({ "albums": [album["id"]], "set": { "year": 1999 } })).await;
    assert_eq!(banks.left_to_follow(), 0, "gone over");
    assert!(std::fs::read(&file).unwrap() == kept, "nothing to move");
    assert!(std::fs::read(&before).unwrap() == previous, "nothing written");

    let undone = s.ok(reqwest::Method::POST, "/api/undo", json!({ "tokens": [out["undo"]] })).await;
    assert_eq!(in_order(row_ids(&s.get(&rows).await)), in_order([vec![banks.stays.clone()], banks.others.clone()].concat()), "put back, once: {undone}");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// A track excluded before its id is taken stays out of every crate, for each account that excluded it: one whose store is open,
/// and one that has not been in since the start, whose file is all there is. A device keeps that list, so it is told.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_track_excluded_stays_out_when_its_id_is_taken_end_to_end() {
    let (s, root, banks) = two_banks("taken-excluded").await;
    s.ok(reqwest::Method::POST, "/api/users", json!({ "name": "ann", "password": "ann's password", "admin": true })).await;
    let ann = s.sign_in("ann", "ann's password").await;
    ann.ok(reqwest::Method::POST, "/api/users", json!({ "name": "bob", "password": "bob's password" })).await;
    let bob = s.sign_in("bob", "bob's password").await;
    let id = banks.crate_of(&ann).await;
    let rows = format!("/api/crates/{id}/rows");
    ann.ok(reqwest::Method::POST, "/api/excluded", json!({ "ids": [banks.goes] })).await;
    assert_eq!(row_ids(&ann.get(&rows).await).len(), 3, "the other file is another track for now");
    let his = root.join("data/users/bob/crates.json");
    std::fs::create_dir_all(his.parent().unwrap()).unwrap();
    std::fs::write(&his, json!({ "version": 1, "crates": [], "excluded": [banks.goes] }).to_string()).unwrap();

    banks.take(&ann).await;
    assert_eq!(in_order(row_ids(&ann.get(&rows).await)), in_order(banks.others.clone()), "out of every crate, whichever file it is read from");
    assert_eq!(row_ids(&ann.get("/api/excluded").await), [banks.stays.clone()], "on the list that puts it back");
    let file: Value = serde_json::from_slice(&std::fs::read(&his).unwrap()).unwrap();
    assert_eq!(file["excluded"], json!([banks.stays]), "a store nobody opened");
    assert_eq!(row_ids(&bob.get("/api/excluded").await), [banks.stays.clone()]);
    let user = rusqlite::Connection::open(banks.data.join("userdata.sqlite")).unwrap();
    let told: i64 = user.query_row("SELECT count(*) FROM change_log WHERE account = 'bob' AND kind = 'excluded' AND item = 'tracks'", [], |r| r.get(0)).unwrap();
    assert_eq!(told, 1, "his device reads the list again");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// What is seeded by a track goes on being seeded by it once its id is taken: a saved crate, a Sounds like and what is kept by its
/// id, its picture, and a filter.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_seed_follows_its_track_when_its_id_is_taken_end_to_end() {
    let (s, root, banks) = two_banks("taken-seed").await;
    let like = json!({ "axis": "like", "not": false, "value": { "kind": "like", "of": [banks.goes], "count": 20 } });
    let made = s.ok(reqwest::Method::POST, "/api/crates", json!({ "name": "Like it", "conditions": [like] })).await;
    let saved = format!("/api/crates/{}", made["id"].as_str().unwrap());
    let (before, after) = (format!("like:{}", banks.goes), format!("like:{}", banks.stays));
    s.ok(reqwest::Method::POST, &format!("/api/crates/{before}/left-out"), json!({ "ids": [banks.others[0]] })).await;
    let picture = std::fs::read(banks.second.parent().unwrap().join("music/Various Artists/Club Classics (2001)/cover.jpg")).unwrap();
    let sent = s.http.put(format!("{}/api/crates/{before}/image", s.base)).body(picture).send().await.unwrap();
    assert!(sent.status().is_success(), "{}", sent.status());
    let shown = s.anyone().bytes(&format!("/api/crates/{before}/image")).await.1;
    let filters = json!({ "tracks": { "on": [], "conditions": [like] } });
    s.ok(reqwest::Method::PUT, "/api/me/preferences/filters", json!({ "value": filters, "changed": 1 })).await;

    banks.take(&s.anyone()).await;
    let got = s.get(&saved).await;
    assert_eq!(got["conditions"][0]["value"]["of"], json!([banks.stays]), "{got}");
    assert_eq!(got["names"][&banks.stays]["name"], "Far Bank", "named, as an id with a track behind it");
    assert_eq!(row_ids(&s.get(&format!("/api/crates/{after}/left-out")).await), banks.others[..1], "what was turned down for it");
    let store: Value = serde_json::from_slice(&std::fs::read(root.join("data/crates.json")).unwrap()).unwrap();
    assert_eq!(store["left_out"].as_object().unwrap().keys().collect::<Vec<_>>(), [&after], "no key left behind");
    assert!(s.anyone().bytes(&format!("/api/crates/{after}/image")).await.1 == shown, "its picture");
    let prefs = s.get("/api/me/preferences").await;
    assert_eq!(prefs["values"]["filters"]["value"]["tracks"]["conditions"][0]["value"]["of"], json!([banks.stays]), "{prefs}");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// An artist's id for life on a running server: what the library serves, the panel's heart by name, an artist seed's crate
/// by id, and all of it the same after a restart. A heart on a name no file credits yet is kept too, as it was under the hash.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn an_artist_is_one_id_by_name_and_by_number_end_to_end() {
    let root = temp("artist-id");
    let (music, data) = (root.join("music"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_fixtures(&music);
    settings_0_15(&data, &music.to_string_lossy());
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.until("the fixture library read", Duration::from_secs(120), || async { s.albums().await.len() == 12 }).await;
    // A heart is an account's.
    s.ok(reqwest::Method::POST, "/api/users", json!({ "name": "ann", "password": "ann's password", "admin": true })).await;
    let ann = s.sign_in("ann", "ann's password").await;

    let served = |name: &str| -> String {
        let db = rusqlite::Connection::open(data.join("library.sqlite")).unwrap();
        db.query_row("SELECT id FROM artists WHERE name = ?1", [name], |r| r.get(0)).unwrap()
    };
    let harbour = served("Glass Harbour");
    assert!(harbour.strip_prefix("ar-").is_some_and(|n| n.parse::<i64>().is_ok()), "a number: {harbour}");
    let heart = |name: &str, on: bool| format!("/api/artists/cratarr/favorite?name={}&on={on}", name.replace(' ', "%20"));
    let hearted = |name: &str| format!("/api/artists/cratarr/favorite?name={}", name.replace(' ', "%20"));
    assert_eq!(ann.get(&hearted("Glass Harbour")).await["favorite"], false);
    ann.ok(reqwest::Method::POST, &heart("Glass Harbour", true), json!({})).await;
    ann.ok(reqwest::Method::POST, &heart("Nobody Yet", true), json!({})).await;
    assert_eq!(ann.get(&hearted("GLASS HARBOUR")).await["favorite"], true, "any case is the same name");
    assert_eq!(ann.get(&hearted("Nobody Yet")).await["favorite"], true);
    assert_eq!(ann.get(&hearted("Never Asked")).await["favorite"], false);
    let stars = |item: &str| -> i64 {
        let db = rusqlite::Connection::open(data.join("userdata.sqlite")).unwrap();
        db.query_row("SELECT count(*) FROM annotation WHERE kind = 'artist' AND item = ?1 AND starred = 1", [item], |r| r.get(0)).unwrap()
    };
    assert_eq!(stars(&harbour), 1, "by the id the library serves");

    // An artist seed finds their tracks through the catalogue's ids, as it found them by the name's hash.
    let rows = ann.get(&format!("/api/crates/like:{harbour}/rows")).await;
    assert!(!rows["rows"].as_array().unwrap().is_empty(), "{rows}");
    s.stop();

    let s = Server::start(&data).await;
    let ann = s.sign_in("ann", "ann's password").await;
    assert_eq!(served("Glass Harbour"), harbour, "for life");
    assert_eq!(ann.get(&hearted("Glass Harbour")).await["favorite"], true);
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// A word of `text` that is an id from before 0.16: `al-` or `ar-` and 22 letters and digits, all of them. A record's or an
/// artist's id for life is a number, and a track's has no hash to look for.
fn hashes_in(text: &str) -> Vec<String> {
    let word = |c: char| c.is_ascii_alphanumeric();
    let mut out = Vec::new();
    for prefix in ["al-", "ar-"] {
        for (at, _) in text.match_indices(prefix) {
            let rest = &text[at + prefix.len()..];
            if !text[..at].ends_with(word) && rest.chars().take_while(|c| word(*c)).count() == 22 {
                out.push(text[at..at + prefix.len() + 22].to_string());
            }
        }
    }
    out
}

/// Every text in a database, each with its table: what a scan for an id goes over, as no list of tables can be left short.
fn every_text(file: &Path) -> Vec<(String, String)> {
    let db = rusqlite::Connection::open(file).unwrap();
    let tables: Vec<String> = db.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '%_fts%'").unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    let mut out = Vec::new();
    for table in tables {
        let mut st = db.prepare(&format!("SELECT * FROM \"{table}\"")).unwrap();
        let columns = st.column_count();
        let rows = st.query_map([], |r| Ok((0..columns).filter_map(|i| r.get::<_, String>(i).ok()).collect::<Vec<String>>())).unwrap();
        out.extend(rows.map(Result::unwrap).flatten().map(|text| (table.clone(), text)));
    }
    out
}

/// What each account holds, by (user, kind, item): its plays, own plays, rating and star.
type Held = std::collections::BTreeMap<(String, String, String), (i64, i64, i64, bool)>;

fn held(data: &Path) -> Held {
    let db = rusqlite::Connection::open(data.join("userdata.sqlite")).unwrap();
    let mut st = db.prepare("SELECT user, kind, item, play_count, own_count, rating, starred FROM annotation").unwrap();
    st.query_map([], |r| Ok(((r.get(0)?, r.get(1)?, r.get(2)?), (r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))).unwrap().map(Result::unwrap).collect()
}

/// Every play there is: (who, when, its event, its song).
fn plays(data: &Path) -> Vec<(String, i64, String, String)> {
    let db = rusqlite::Connection::open(data.join("userdata.sqlite")).unwrap();
    let mut st = db.prepare("SELECT user, at, event, song FROM plays ORDER BY event").unwrap();
    st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).unwrap().map(Result::unwrap).collect()
}

/// A track of every file, by (folder, path): what the merge moves.
fn tracks_of_files(db: &rusqlite::Connection) -> Vec<(i64, String, i64)> {
    db.prepare("SELECT folder, rel, track FROM files ORDER BY folder, rel").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect()
}

/// A row of a report's `tables`.
fn table_of<'a>(part: &'a Value, name: &str) -> &'a Value {
    part["tables"].as_array().unwrap().iter().find(|t| t["name"] == name).unwrap_or_else(|| panic!("no {name} in {part}"))
}

/// One album no tag makes any more: a retag long ago left it under a hash nothing can read back.
const ORPHAN: &str = "al-0retagged0long0since00";

/// The migration end to end (decision 0101 points 21 and 28, C35): 0.15's data through a real first start, on real files, by the
/// real binary. What its user data named is what it names now, every play, heart and rating told over and each table's
/// rows accounted for; nothing that had somewhere to go is left under an old id, in a table or a store, whatever table it is; a
/// row that cannot move stays, and is counted; an old link opens its record; a restart changes nothing.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn the_data_of_0_15_moves_onto_its_records_at_its_first_start_end_to_end() {
    let root = temp("data-0-15");
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_layout(&library);
    data_0_15(&library, &data);
    let user = rusqlite::Connection::open(data.join("userdata.sqlite")).unwrap();
    user.execute("INSERT INTO annotation (user, item, kind, play_count, rating, starred, starred_at, own_count) VALUES ('ann', ?1, 'album', 7, 2, 1, 1760000000, 7)", [ORPHAN]).unwrap();
    drop(user);
    let db = rusqlite::Connection::open(data.join("catalogue.sqlite")).unwrap();
    let left = tracks_of_files(&db);
    let present = db.query_row(&format!("SELECT count(*) FROM files WHERE {PRESENT}"), [], |r| r.get::<_, i64>(0)).unwrap() as usize;
    drop(db);
    assert_eq!(present, audio_under(&library.join("music")).len() + audio_under(&library.join("incoming")).len() + audio_under(&library.join("second")).len(), "a row for every file");
    let (held_before, plays_before) = (held(&data), plays(&data));

    let s = Server::start(&data).await;
    let ann = s.sign_in("ann", "ann's password").await;
    ann.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    // Every library read: the albums' titles, 20 of the layout's 22, as its two accounts' own have no place in 0.15.
    let titles = |albums: Vec<Value>| albums.iter().filter_map(|a| a["title"].as_str().map(str::to_string)).collect::<std::collections::BTreeSet<String>>();
    s.until("every library read", Duration::from_secs(180), || async { titles(ann.albums().await).len() == 20 }).await;
    let both = |a: &Value| a["title"] == "Both Shores" && a["copies"].as_array().is_some_and(|c| c.len() == 3);
    s.until("the record's three copies", Duration::from_secs(120), || async { ann.albums().await.iter().any(both) }).await;

    // The catalogue: the main library's tracks stand, the other two copies on them, and nothing new, nothing else moved.
    let db = rusqlite::Connection::open(data.join("catalogue.sqlite")).unwrap();
    let taken: std::collections::HashMap<i64, i64> = db.prepare("SELECT old, id FROM track_moved").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
    assert_eq!(taken, [(19, 48), (18, 47), (17, 46), (8, 48), (7, 47), (6, 46), (15, 41), (14, 40), (13, 39)].into(), "Music's ids survive, each track by its place");
    let followed: Vec<(i64, String, i64)> = left.iter().map(|(folder, rel, track)| (*folder, rel.clone(), *taken.get(track).unwrap_or(track))).collect();
    assert_eq!(tracks_of_files(&db), followed, "nothing new, and nothing moved but what the merge moved");
    assert_eq!(db.query_row("SELECT count(*) FROM tracks", [], |r| r.get::<_, i64>(0)).unwrap(), left.len() as i64 + 1, "a taken id keeps its row, and the retired one stands");
    assert_eq!(db.query_row("SELECT count(*) FROM track_moved WHERE followed = 0", [], |r| r.get::<_, i64>(0)).unwrap(), 0, "all followed");

    // The report, and each table's rows told over: what met another is counted, and a row goes nowhere else.
    let report = ann.get("/api/admin/migration").await;
    let part = report["parts"]["records"].clone();
    assert_eq!((part["tracks_merged"].as_u64(), part["bases_on_two_records"].as_u64()), (Some(9), Some(2)), "{part}");
    assert_eq!(part["copies"].as_u64().unwrap(), part["records"].as_u64().unwrap() + 3, "Both Shores' three copies are one record, and Short Measure's two: {part}");
    assert!(part["artists"].as_u64().unwrap() >= 4, "{part}");
    for row in part["tables"].as_array().unwrap() {
        let (name, before, after, merged) = (&row["name"], row["before"].as_u64().unwrap(), row["after"].as_u64().unwrap(), row["merged"].as_u64().unwrap());
        // What goes by a base on two records goes to both, and the change log is told or cleared: no row of those can be lost.
        let more = ["album_tags", "record_overrides", "words: albums", "words_asked: albums", "discogs_facts: albums", "change_log: tracks", "change_log: albums"];
        if !more.iter().any(|m| name == m) {
            assert_eq!(before - merged, after, "{row}");
        }
    }
    let unmatched: Vec<&Value> = part["unmatched"].as_array().unwrap().iter().filter(|u| u["count"].as_u64() != Some(0)).collect();
    assert_eq!(unmatched.len(), 1, "nothing unmatched that should match: {unmatched:?}");
    assert_eq!((&unmatched[0]["what"], unmatched[0]["count"].as_u64(), &unmatched[0]["sample"]), (&json!("annotation: albums"), Some(1), &json!([ORPHAN])));
    assert_eq!(table_of(&part, "plays")["moved"], 2, "two plays were on a copy's track");
    assert_eq!((table_of(&part, "annotation: tracks")["before"].as_u64(), table_of(&part, "annotation: tracks")["after"].as_u64()), (Some(6), Some(5)));
    // A library that changed place, either way round: what 0.15 left at the folder it had lies at the folder it has.
    let folder_of = |table: &str, file: &str| db.query_row(&format!("SELECT folder FROM {table} WHERE rel LIKE '%/{file}'"), [], |r| r.get::<_, i64>(0)).unwrap();
    for table in ["analysis_shape", "beat_this"] {
        assert_eq!((folder_of(table, "01 Far Bank.flac"), folder_of(table, "01 Just Arrived.mp3")), (2, 1), "{table}");
    }
    assert_eq!((part["shapes_moved"].as_u64(), part["beats_moved"].as_u64()), (Some(2), Some(2)), "{part}");
    assert_eq!(report["parts"]["libraries"]["sources"]["cratarr"], json!({ "main": 1, "incoming": 2 }));
    assert_eq!((report["parts"]["playlists"]["playlists"].as_u64(), report["parts"]["playlists"]["lines"].as_u64()), (Some(2), Some(4)), "{report}");

    // Every play, heart and rating, by the map as the owner's release run reads it: added up, the better rating, the first star.
    let moved = ann.get("/api/admin/migration/moved").await;
    let now = |id: &str| ["albums", "artists", "tracks"].iter().find_map(|kind| moved[*kind][id].as_str()).unwrap_or(id).to_string();
    let mut told = Held::new();
    for ((who, kind, item), (played, own, rating, star)) in &held_before {
        let row = told.entry((who.clone(), kind.clone(), now(item))).or_default();
        *row = (row.0 + played, row.1 + own, row.2.max(*rating), row.3 || *star);
    }
    assert_eq!(held(&data), told, "each account's, told over");
    let expected: Vec<(String, i64, String, String)> = plays_before.iter().map(|(who, at, event, song)| (who.clone(), *at, event.clone(), now(song))).collect();
    assert_eq!(plays(&data), expected, "every play once, on the track it is now");

    // One album's two ids, from before its library changed place and after, either way round: one row of each, with what both held.
    let user = rusqlite::Connection::open(data.join("userdata.sqlite")).unwrap();
    for (who, plays, has, had) in [("ann", 4, "al-4iYsod5k8p0gnwEbQmevTB", "al-16xHJujSyQP4kXgPnlrD5a"), ("bob", 3, "al-5CBklbfbpDtbwWYNnJiHHl", "al-5kyy3QkYKUCRrEIvEzhxHQ")] {
        let record = now(has);
        assert_eq!(now(had), record, "the place it had");
        let of_it = |sql: &str| user.query_row(sql, [&record, &who.to_string()], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!(of_it("SELECT count(*) FROM annotation WHERE item = ?1 AND user = ?2"), 1);
        assert_eq!((of_it("SELECT play_count FROM annotation WHERE item = ?1 AND user = ?2"), of_it("SELECT starred FROM annotation WHERE item = ?1 AND user = ?2")), (plays, 1), "the plays of one, the heart of the other");
        assert_eq!((of_it("SELECT count(*) FROM added WHERE album = ?1 AND ?2 <> ''"), of_it("SELECT at FROM added WHERE album = ?1 AND ?2 <> ''")), (1, 1760880000), "the earlier");
        assert_eq!((of_it("SELECT count(*) FROM album_complete WHERE album = ?1 AND ?2 <> ''"), of_it("SELECT found IS NULL FROM album_complete WHERE album = ?1 AND ?2 <> ''")), (1, 1), "the look under the id it has now");
    }
    drop(user);

    // Nothing that has somewhere to go stays behind, in any table of the user data or any store. Not named here: a scan has no list.
    let old: Vec<(String, String)> = every_text(&data.join("userdata.sqlite")).into_iter().filter(|(table, _)| table != "migration_report").flat_map(|(table, text)| hashes_in(&text).into_iter().map(move |id| (table.clone(), id))).collect();
    assert_eq!(old, [("annotation".to_string(), ORPHAN.to_string())], "the one no tag makes, kept");
    for store in ["crates.json", "users/bob/crates.json", "queues.json", "track-intake.json", "convert-queue.json"] {
        let text = std::fs::read_to_string(data.join(store)).unwrap();
        assert_eq!(hashes_in(&text), Vec::<String>::new(), "{store}");
    }
    let picture = format!("x{}.jpg", format!("like:{}", now("al-4Jx9dQszNKgnbo4mZtic7y")).bytes().map(|b| format!("{b:02x}")).collect::<String>());
    assert!(data.join("crate-images").join(picture).is_file(), "a crate's picture follows its seed");
    assert_eq!(every_text(&data.join("journal.sqlite")).iter().flat_map(|(_, text)| hashes_in(text)).collect::<Vec<_>>(), Vec::<String>::new());
    let gone = ["tr-19", "tr-18", "tr-17", "tr-8", "tr-7", "tr-6", "tr-15", "tr-14", "tr-13"];
    let names_one = |text: &str| gone.iter().any(|id| text.split(|c: char| !c.is_ascii_alphanumeric() && c != '-').any(|word| word == *id));
    // A device told of a heart hears it went, by the id it knew, and a play keeps the event it was told by.
    let kept = |table: &str, text: &str| table == "migration_report" || table == "change_log" || text.starts_with("backfill:");
    assert!(!every_text(&data.join("userdata.sqlite")).iter().any(|(table, text)| !kept(table, text) && names_one(text)), "an id the merge took");
    let sync: String = rusqlite::Connection::open(data.join("playlist-sync.sqlite")).unwrap().query_row("SELECT base FROM links", [], |r| r.get(0)).unwrap();
    assert_eq!(sync, "[83,48,87]", "an agreed list, place by place");

    // A two-copy record is listed once, on one set of tracks.
    let shores: Vec<Value> = ann.albums().await.into_iter().filter(|a| a["title"] == "Both Shores").collect();
    assert_eq!(shores.len(), 1, "{shores:?}");
    let record = shores[0]["id"].as_str().unwrap().to_string();
    assert_eq!(now("al-4Jx9dQszNKgnbo4mZtic7y"), record, "Music's id");
    assert_eq!(now("al-0K3QLxUBwKPLshc0VyNQJ7"), record, "Incoming's");
    let songs = ann.get(&format!("/api/albums/cratarr/{record}")).await;
    assert_eq!(songs["rows"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap()).collect::<Vec<_>>(), ["tr-48", "tr-47", "tr-46"]);

    // A crate that named both ids has them as one seed, and shows what it is.
    let def = ann.get("/api/crates/c-shores").await;
    assert_eq!((&def["conditions"][0]["value"]["of"], &def["names"][&record]["name"]), (&json!([record]), &json!("Both Shores")), "{def}");
    let left_out = ann.get("/api/crates/c-shores/left-out").await;
    assert_eq!(left_out["rows"].as_array().unwrap().iter().map(|r| r["name"].as_str().unwrap()).collect::<Vec<_>>(), ["Neap"], "a line is its track, and reads as one: {left_out}");
    let bob = s.sign_in("bob", "bob's password").await;
    let def = bob.get("/api/crates/c-rooms").await;
    assert!(hashes_in(&def["conditions"].to_string()).is_empty(), "{def}");

    // Playlists come in on the tracks that stood, and a line on what retired stays, missing, saying what it was.
    let state = ann.get("/api/state").await;
    let mix = state["playlists"].as_array().unwrap().iter().find(|p| p["name"] == "Mix").cloned().expect("Mix brought in");
    let lines = ann.get(&format!("/api/playlists/{}/rows", mix["id"].as_str().unwrap())).await;
    let rows = lines["rows"].as_array().unwrap();
    assert_eq!(rows.iter().map(|r| r["missing"].as_bool().unwrap()).collect::<Vec<_>>(), [false, false, true]);
    assert_eq!((rows[1]["id"].as_str(), rows[1]["name"].as_str()), (Some("tr-48"), Some("Landing")));
    assert!(rows[2]["file"].as_str().unwrap().ends_with("Last Seen.flac"), "{}", rows[2]);
    let crates = ann.get("/api/crates").await;
    assert!(crates.as_array().unwrap().iter().any(|c| c["name"] == "Like both shores"), "{crates}");

    // Started again: nothing moves, the report stands as it was, and nothing that was moved reads as old.
    let (rows_then, plays_then) = (held(&data), plays(&data));
    let files_then = tracks_of_files(&db);
    s.stop();
    let s = Server::start(&data).await;
    let ann = s.sign_in("ann", "ann's password").await;
    s.until("the library read again", Duration::from_secs(120), || async { ann.albums().await.iter().any(both) }).await;
    assert_eq!(ann.get("/api/admin/migration").await["parts"]["records"], part, "written once");
    assert_eq!((held(&data), plays(&data), tracks_of_files(&db)), (rows_then, plays_then, files_then));
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// A link kept from before 0.16 opens what it named, as the map says: a copy's own id, the id a Keep's album had in Incoming, which
/// no album has now, and a Sounds like by its seed. One the map does not hold finds its fate as before.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn an_old_album_link_opens_the_record_end_to_end() {
    let root = temp("old-links");
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_layout(&library);
    data_0_15(&library, &data);
    let s = Server::start(&data).await;
    let ann = s.sign_in("ann", "ann's password").await;
    ann.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    let both = |a: &Value| a["title"] == "Both Shores" && a["copies"].as_array().is_some_and(|c| c.len() == 3);
    s.until("the record's three copies", Duration::from_secs(180), || async { ann.albums().await.iter().any(both) }).await;
    let record = ann.albums().await.into_iter().find(both).unwrap()["id"].as_str().unwrap().to_string();

    let view = ann.get("/api/albums/cratarr/al-0K3QLxUBwKPLshc0VyNQJ7").await;
    assert_eq!((view["id"].as_str(), view["title"].as_str()), (Some(&*record), Some("Both Shores")), "Incoming's copy: {view}");
    assert_eq!(view["rows"].as_array().unwrap().len(), 3);
    let tide = ann.albums().await.into_iter().find(|a| a["title"] == "Tide Tables").unwrap()["id"].as_str().unwrap().to_string();
    let moved = ann.get("/api/admin/migration/moved").await;
    let worked_out: Vec<&String> = moved["albums"].as_object().unwrap().iter().filter(|(old, now)| now.as_str() == Some(&*tide) && *old != "al-4FwVmWLNdYpZZSEtkan6Uy").map(|(old, _)| old).collect();
    assert_eq!(worked_out.len(), 2, "what it would have been in each Incoming: {moved}");
    let view = ann.get(&format!("/api/albums/cratarr/{}", worked_out[0])).await;
    assert_eq!((view["id"].as_str(), view["title"].as_str()), (Some(&*tide), Some("Tide Tables")), "a Keep's old id: {view}");

    let def = ann.get("/api/crates/like:al-0K3QLxUBwKPLshc0VyNQJ7@50").await;
    assert_eq!((def["id"].as_str(), def["name"].as_str()), (Some(&*format!("like:{record}@50")), Some("Sounds like Both Shores")), "{def}");
    assert_eq!(ann.send(reqwest::Method::GET, "/api/albums/cratarr/al-5vcUvYmuWJAgyxLXEUmaPz", Value::Null).await.1["rows"].as_array().map(Vec::len).unwrap_or(0), 0, "one no map holds opens nothing else");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// A start that cannot finish ends by itself, and not well: what it said.
fn start_refused(data: &Path) -> String {
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut child = Command::new(BIN)
        .env("CRATARR_DATA_DIR", data)
        .env("CRATARR_BIND", format!("127.0.0.1:{port}"))
        .env("RUST_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the server starts");
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > Duration::from_secs(60) {
            child.kill().ok();
            panic!("it went on as if nothing stopped it");
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    assert!(!status.success(), "stopped, and said so");
    let mut said = String::new();
    std::io::Read::read_to_string(&mut child.stderr.take().unwrap(), &mut said).unwrap();
    said
}

/// A real first start stopped at every place one can stop: a library not mounted, then each database and store in turn unreadable
/// as its step comes to it, part of the stores moved, and the last statement of all refused, so the user data falls back whole. Each
/// time it says why and goes no further, nothing reads as done, and a start that can finish leaves what one that was never stopped
/// leaves, its report too.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_first_start_stopped_at_any_step_finishes_on_restart_end_to_end() {
    let root = temp("first-start-stops");
    let (library, straight, data) = (root.join("library"), root.join("straight"), root.join("data"));
    std::fs::create_dir_all(&straight).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    make_layout(&library);

    // What it comes to with nothing in its way: the first start is over by the time the server answers.
    data_0_15(&library, &straight);
    let s = Server::start(&straight).await;
    s.stop();
    let report = |data: &Path| -> Value {
        let db = rusqlite::Connection::open(data.join("userdata.sqlite")).unwrap();
        let text: String = db.query_row("SELECT report FROM migration_report WHERE part = 'records'", [], |r| r.get(0)).unwrap();
        serde_json::from_str(&text).unwrap()
    };
    let done = |data: &Path| -> bool {
        let db = rusqlite::Connection::open(data.join("userdata.sqlite")).unwrap();
        db.query_row("SELECT count(*) FROM migration_report WHERE part = 'records'", [], |r| r.get::<_, i64>(0)).unwrap_or(0) > 0
    };
    let merged = |data: &Path| -> i64 { rusqlite::Connection::open(data.join("catalogue.sqlite")).unwrap().query_row("SELECT count(*) FROM track_moved", [], |r| r.get(0)).unwrap_or(0) };
    let expected = report(&straight);

    data_0_15(&library, &data);
    let (second, away) = (library.join("second"), root.join("away"));
    std::fs::rename(&second, &away).unwrap();
    let said = start_refused(&data);
    assert!(said.contains(&*second.to_string_lossy()), "it says which library: {said}");
    assert_eq!((done(&data), merged(&data)), (false, 0), "and nothing moved");
    std::fs::rename(&away, &second).unwrap();

    // Each file unreadable as its step comes to it: the step before is done, and stays done.
    let unreadable = |file: &str| {
        let (file, kept) = (data.join(file), std::fs::read(data.join(file)).unwrap());
        std::fs::write(&file, b"nothing a server could read").unwrap();
        let said = start_refused(&data);
        assert_eq!((done(&data), merged(&data)), (false, 9), "{}: {said}", file.display());
        std::fs::write(&file, kept).unwrap();
    };
    unreadable("journal.sqlite");
    unreadable("playlist-sync.sqlite");
    unreadable("convert-queue.json");
    assert!(std::fs::read_to_string(data.join("crates.json")).unwrap().contains("tr-82"), "the stores before it are moved");
    unreadable("track-intake.json");

    // The last statement of all refused: the whole of the user data's move falls back with it.
    let user = rusqlite::Connection::open(data.join("userdata.sqlite")).unwrap();
    user.execute_batch("CREATE TRIGGER no_report BEFORE INSERT ON migration_report WHEN NEW.part = 'records' BEGIN SELECT RAISE(ABORT, 'stopped'); END;").unwrap();
    let said = start_refused(&data);
    assert!(said.contains("stopped"), "{said}");
    assert_eq!(user.query_row("SELECT count(*) FROM annotation WHERE item = 'al-4FwVmWLNdYpZZSEtkan6Uy'", [], |r| r.get::<_, i64>(0)).unwrap(), 1, "still under its old id");
    assert!(!done(&data));
    user.execute_batch("DROP TRIGGER no_report").unwrap();
    drop(user);

    let s = Server::start(&data).await;
    s.stop();
    // The stores come as they were moved: a stop part way through them leaves the ones before counted at none on the next go.
    let plain = |mut part: Value| {
        for t in part["tables"].as_array_mut().unwrap() {
            if t["name"].as_str().is_some_and(|n| n.ends_with(".json") || n == "crate-images") {
                t["moved"] = json!(0);
            }
        }
        part["unmatched"].as_array_mut().unwrap().retain(|u| u["count"].as_u64() != Some(0) && !u["what"].as_str().is_some_and(|n| n.ends_with(".json") || n == "crate-images"));
        part
    };
    assert_eq!(plain(report(&data)), plain(expected), "as if nothing had stopped it");
    let crates = |data: &Path| std::fs::read_to_string(data.join("crates.json")).unwrap();
    assert_eq!(crates(&data), crates(&straight));
    std::fs::remove_dir_all(&root).ok();
}

/// CR14: who hears what, on real files, by a session and by Subsonic's own sign-in, by the bytes: an account's own copy for its
/// owner where it is the better one, the kept library's for anyone else, Incoming's only when it is asked for, and a copy deleted
/// leaves the other playing. The own copy is the better, so this holds however a viewer's own music ranks against a kept one.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn who_hears_what_by_the_bytes_on_both_streams_end_to_end() {
    let root = temp("who-hears");
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_layout(&library);
    let (kept, waiting, own) = (library.join("incoming"), library.join("second"), library.join("own"));
    // Kept: the MP3s. Her own: Music's FLACs, in place of her MP3s. Waiting: the other FLACs.
    let hers = own.join("ann").join(SHORES);
    std::fs::remove_dir_all(&hers).unwrap();
    std::fs::create_dir_all(&hers).unwrap();
    for file in audio_under(&library.join("music").join(SHORES)) {
        std::fs::copy(library.join("music").join(SHORES).join(&file), hers.join(&file)).unwrap();
    }
    settings(&data, &[(&kept, "main"), (&waiting, "incoming")], &own);
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.ok(reqwest::Method::POST, "/api/users", json!({ "name": "ann", "password": "ann's password", "admin": true })).await;
    let ann = s.sign_in("ann", "ann's password").await;
    ann.ok(reqwest::Method::POST, "/api/users", json!({ "name": "bob", "password": "bob's password" })).await;
    let bob = s.sign_in("bob", "bob's password").await;
    let three = |a: &Value| a["title"] == "Both Shores" && a["copies"].as_array().is_some_and(|c| c.len() == 3);
    s.until("the record's three copies", Duration::from_secs(120), || async { ann.albums().await.iter().any(three) }).await;
    let album = ann.albums().await.into_iter().find(three).unwrap();
    let id = album["id"].as_str().unwrap().to_string();
    let row = ann.get(&format!("/api/albums/cratarr/{id}")).await;
    let song = row["rows"][0]["id"].as_str().unwrap().to_string();

    let first_of = |dir: PathBuf| std::fs::read(dir.join(&audio_under(&dir)[0])).unwrap();
    let (mp3, flac, other) = (first_of(kept.join(SHORES)), first_of(hers.clone()), first_of(waiting.join(SHORES)));
    assert!(mp3 != flac && flac != other && mp3 != other, "three copies, told apart by their bytes");
    let (stream, file) = (format!("/api/stream/cratarr/{song}"), format!("/api/file/cratarr/{song}"));
    let subsonic = |who: &str, password: &str| format!("/rest/stream?id={song}&u={who}&p={}&v=1.16.1&c=e2e", password.replace(' ', "%20").replace('\'', "%27"));
    let (for_ann, for_bob) = (subsonic("ann", "ann's password"), subsonic("bob", "bob's password"));

    // `/api/stream` makes a FLAC into what a browser's player takes, so it is told by its kind, and the file by its bytes.
    assert!(ann.bytes(&file).await.1 == flac && !is_mp3(&ann.bytes(&stream).await.1), "her own, the better");
    assert!(bob.bytes(&file).await.1 == mp3 && is_mp3(&bob.bytes(&stream).await.1), "the kept library's for him, her own music being off");
    assert!(s.anyone().bytes(&for_ann).await.1 == flac, "the same by Subsonic's sign-in");
    assert!(s.anyone().bytes(&for_bob).await.1 == mp3);

    // Incoming's only when asked for, and never a thing the next play remembers.
    let from = id_of(&waiting).replace('/', "%2F");
    assert!(!is_mp3(&bob.bytes(&format!("{stream}?from={from}")).await.1), "named, it plays");
    assert!(is_mp3(&bob.bytes(&stream).await.1), "and the next play is the best kept one again");
    assert!(other != mp3 && bob.bytes(&file).await.1 == mp3);

    // Her copy deleted: the kept one plays on for her, and his never stopped.
    ann.ok(reqwest::Method::POST, &format!("/api/albums/cratarr/{id}/delete-own"), json!({ "from": id_of(&own.join("ann")) })).await;
    s.until("her copy gone", Duration::from_secs(120), || async { ann.bytes(&file).await.1 == mp3 }).await;
    assert!(is_mp3(&ann.bytes(&stream).await.1));
    assert!(s.anyone().bytes(&for_ann).await.1 == mp3, "by Subsonic's too");
    assert!(bob.bytes(&file).await.1 == mp3);
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// The first start of `scripts/dev-instance.sh`, step by step, as this suite cannot run the script: its account in the
/// data folder before anything starts, so sign-in is on as in any new folder, and off only by the switch, with the password.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_development_instance_signs_in_once_to_switch_sign_in_off_end_to_end() {
    let root = temp("dev-instance");
    let (music, data) = (root.join("music"), root.join("data"));
    std::fs::create_dir_all(&music).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    settings_0_15(&data, music.to_str().unwrap());
    std::fs::write(data.join("subsonic-users.json"), r#"[{ "name": "owner", "password": "0f1e2d3c", "admin": true }]"#).unwrap();

    let s = Server::start(&data).await;
    assert_eq!(s.send(reqwest::Method::GET, "/api/albums", json!({})).await.0, 401, "an account here says nothing of the sign-in");
    assert_eq!(s.send(reqwest::Method::GET, "/api/health", json!({})).await.0, 200, "what the script waits for");
    let owner = s.sign_in("owner", "0f1e2d3c").await;
    owner.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    assert_eq!(owner.send(reqwest::Method::PUT, "/api/auth/required", json!({ "on": false, "password": "another" })).await.0, 403);
    owner.ok(reqwest::Method::PUT, "/api/auth/required", json!({ "on": false, "password": "0f1e2d3c" })).await;
    assert_eq!(s.send(reqwest::Method::GET, "/api/albums", json!({})).await.0, 200, "nobody signs in now");
    s.stop();

    // The instance restarts with its machine, and the script starts it anew.
    let s = Server::start(&data).await;
    assert_eq!(s.send(reqwest::Method::GET, "/api/albums", json!({})).await.0, 200);
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
    make_layout(&library);
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

    // A record is one entry whichever libraries hold it, its copies in the order they play for whoever asks, each with
    // what that viewer may do with it. Two records of one name stay two.
    let (music, second, incoming) = (library.join("music"), library.join("second"), library.join("incoming"));
    let path = |p: &Path| p.to_string_lossy().to_string();
    let named = |albums: Vec<Value>, title: &str| albums.into_iter().filter(|a| a["title"] == title).collect::<Vec<Value>>();
    let copies = |a: &Value| a["copies"].as_array().cloned().unwrap_or_default();
    s.until("the record's four copies", Duration::from_secs(120), || async { named(ann.albums().await, "Both Shores").first().is_some_and(|a| copies(a).len() == 4) }).await;
    let shores = named(ann.albums().await, "Both Shores");
    assert_eq!(shores.len(), 1, "one record, listed once: {shores:?}");
    let in_order: Vec<String> = copies(&shores[0]).iter().map(|c| c["library"].as_str().unwrap().to_string()).collect();
    assert!([path(&music), path(&second)].contains(&in_order[0]), "a FLAC first, her own MP3 being worse: {in_order:?}");
    assert_eq!(in_order[2], path(&own.join("ann")), "her own music by its quality");
    assert_eq!(in_order[3], path(&incoming), "Incoming last");
    assert_eq!((copies(&shores[0])[0]["plays"].as_bool(), shores[0]["format"].as_str(), shores[0]["library"].as_str()), (Some(true), Some("FLAC"), Some(&*in_order[0])), "the card is what plays");
    assert_eq!(copies(&shores[0]).iter().filter(|c| c["plays"] == true).count(), 1);
    let of = |list: &[Value], p: &Path| list.iter().find(|c| c["library"].as_str() == Some(&*path(p))).cloned().unwrap();
    assert_eq!(of(&copies(&shores[0]), &music)["deletable"], true, "an administrator deletes in a main library");
    assert_eq!(of(&copies(&shores[0]), &incoming)["keepable"], true);
    assert_eq!(of(&copies(&shores[0]), &music)["keepable"], false, "only an Incoming copy is added");
    assert!(of(&copies(&shores[0]), &incoming)["keep_to"].is_string() && of(&copies(&shores[0]), &music)["keep_to"].is_null());

    let shores = named(bob.albums().await, "Both Shores");
    assert_eq!(shores.len(), 1);
    let theirs = copies(&shores[0]);
    assert_eq!(theirs.len(), 4, "someone's own music is a copy too, off or not");
    assert!([path(&music), path(&second)].iter().any(|p| theirs[0]["library"].as_str() == Some(&**p)), "for bob the best kept copy: {theirs:?}");
    assert_eq!((theirs[0]["plays"].as_bool(), shores[0]["format"].as_str()), (Some(true), Some("FLAC")));
    assert_eq!(of(&theirs, &music)["deletable"], false, "not an administrator");
    assert_eq!(of(&theirs, &incoming)["keepable"], false, "not a request of his");

    let papers = named(bob.albums().await, "No Papers");
    assert_eq!(papers.len(), 1, "no release on either side, one record");
    assert_eq!(copies(&papers[0])[0]["library"].as_str(), Some(&*path(&music)), "a FLAC in Music, over his own MP3");
    assert_eq!(named(ann.albums().await, "Bonus Round").len(), 2, "a bonus track makes another record");
    let pressings = named(ann.albums().await, "Two Pressings");
    assert_eq!(pressings.len(), 2, "two releases are two records");
    assert_ne!(pressings[0]["id"], pressings[1]["id"]);

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

    // Someone's own copy of a record also in Music, deleted from the Users panel by naming their library: Music's stands.
    let (shores, dir) = (ann.albums().await.into_iter().find(|a| a["title"] == "Both Shores").unwrap(), "Salt Meridian/Both Shores (2018)");
    let deleting = format!("/api/albums/cratarr/{}/delete-own", shores["id"].as_str().unwrap());
    let (theirs, kept) = (own.join("ann").join(dir), as_it_lies(&music.join(dir)));
    assert_eq!(ann.send(reqwest::Method::POST, &deleting, json!({ "from": path(&music) })).await.0, 409, "that is no one's own music");
    assert!(theirs.exists());
    ann.ok(reqwest::Method::POST, &deleting, json!({ "from": path(&own.join("ann")) })).await;
    assert!(!theirs.exists(), "the owner's copy went");
    assert!(as_it_lies(&music.join(dir)) == kept, "Music's stands");
    s.stop();

    // A server that asks for a sign-in starts as any other.
    let s = Server::start(&data).await;
    assert_eq!(s.sign_in("bob", "bob's password").await.get("/api/albums").await["sources"][0]["id"], "cratarr");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// The whole index as a device fetches it: (status, its bytes, gzip's or not), `gzip` being what is asked.
async fn index_bytes(who: &Caller<'_>, gzip: bool) -> (u16, Vec<u8>, bool) {
    let mut asked = who.request(reqwest::Method::GET, "/api/index");
    if gzip {
        asked = asked.header(reqwest::header::ACCEPT_ENCODING, "gzip");
    }
    let r = asked.send().await.unwrap();
    let (status, zipped) = (r.status().as_u16(), r.headers().get(reqwest::header::CONTENT_ENCODING).is_some_and(|v| v == "gzip"));
    (status, r.bytes().await.unwrap().to_vec(), zipped)
}

async fn index_of(who: &Caller<'_>) -> cratarr_rules::index::Index {
    let (status, bytes, _) = index_bytes(who, false).await;
    assert_eq!(status, 200);
    cratarr_rules::index::Index::load(&bytes).unwrap()
}

/// Each account fetches every library it can see, what it has switched off too (0101's point 16: switching one on works offline),
/// and is told whose own music is whose, as the rest of the page is.
#[tokio::test]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn two_accounts_fetch_every_library_they_can_see_end_to_end() {
    let root = temp("device-index");
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    assert!(Command::new(BIN).arg("--make-layout").arg(&library).status().unwrap().success(), "the whole layout is made");
    let own = library.join("own");
    settings(&data, &[(&library.join("music"), "main"), (&library.join("second"), "main"), (&library.join("incoming"), "incoming")], &own);
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.ok(reqwest::Method::POST, "/api/users", json!({ "name": "ann", "password": "ann's password", "admin": true })).await;
    let ann = s.sign_in("ann", "ann's password").await;
    ann.ok(reqwest::Method::POST, "/api/users", json!({ "name": "bob", "password": "bob's password" })).await;
    let bob = s.sign_in("bob", "bob's password").await;
    let titles = |albums: Vec<Value>| albums.iter().filter_map(|a| a["title"].as_str().map(str::to_string)).collect::<std::collections::BTreeSet<String>>();
    s.until("every library read", Duration::from_secs(180), || async { titles(ann.albums().await).len() == 22 }).await;
    s.until("the record's four copies", Duration::from_secs(120), || async {
        ann.albums().await.iter().any(|a| a["title"] == "Both Shores" && a["copies"].as_array().is_some_and(|c| c.len() == 4))
    })
    .await;

    // Who has what switched off is no matter here: bob has ann's own music off by default, and he holds it.
    let off = bob.get("/api/me/preferences").await;
    let own_ann = format!("cratarr/{}", own.join("ann").display());
    assert!(off["values"]["hidden_libraries"]["value"].as_array().is_some_and(|v| v.iter().any(|k| k.as_str() == Some(&*own_ann))), "{off}");
    let (of_ann, of_bob) = (index_of(&ann).await.rows(), index_of(&bob).await.rows());
    let listed = |rows: &cratarr_rules::index::Rows| rows.records.iter().map(|r| r.title.clone()).collect::<std::collections::BTreeSet<String>>();
    assert_eq!(listed(&of_ann), titles(ann.albums().await), "every record the Library lists");
    assert_eq!(listed(&of_bob), listed(&of_ann));
    assert!(listed(&of_bob).contains("Own Room"), "ann's own music, off for bob and held for the day he turns it on");

    // Every library, by the id the rows name them by, as the page names them.
    let library_of = |rows: &cratarr_rules::index::Rows, key: &Path| rows.libraries.iter().find(|l| l.key == key.to_string_lossy()).cloned().unwrap();
    assert_eq!(of_ann.libraries.len(), 5, "{:?}", of_ann.libraries);
    let (theirs, music) = (library_of(&of_bob, &own.join("ann")), library_of(&of_bob, &library.join("music")));
    assert_eq!((theirs.owner.as_deref(), theirs.kind), (Some("ann"), cratarr_rules::index::Kind::Own));
    assert_eq!((music.owner.as_deref(), music.kind, music.order), (None, cratarr_rules::index::Kind::Main, 0));
    assert_eq!(library_of(&of_ann, &library.join("incoming")).kind, cratarr_rules::index::Kind::Incoming);

    // A record in every library holds a copy in each, in the order it plays.
    let shores = of_bob.records.iter().find(|r| r.title == "Both Shores").unwrap();
    assert_eq!(shores.copies.len(), 4);
    assert_eq!(shores.copies.last().unwrap().library, library_of(&of_bob, &library.join("incoming")).id, "Incoming last");
    let track = of_bob.tracks.iter().find(|t| t.record == shores.id).unwrap();
    assert_eq!(track.facts.folders.len(), 4, "a track says where it lies");
    assert_eq!(track.facts.lines.len(), 3, "and by which line");

    // Sent packed to whoever takes it, and only to them.
    let (_, plain, zipped) = index_bytes(&ann, false).await;
    assert!(!zipped);
    let (status, packed, zipped) = index_bytes(&ann, true).await;
    assert!(status == 200 && zipped && packed.len() < plain.len(), "{} of {}", packed.len(), plain.len());
    let mut back = Vec::new();
    std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&packed[..]), &mut back).unwrap();
    assert!(back == plain, "packed, it is the same index");
    assert_eq!(index_bytes(&s.anyone(), false).await.0, 401, "no session, no library");

    // The sounds apart, by a route of their own: AudioMuse has heard nothing here, so they say that.
    let (status, sounds) = ann.bytes("/api/index/sounds").await;
    assert_eq!(status, 200);
    let sounds = cratarr_rules::index::Sounds::load(&sounds).unwrap();
    assert!(sounds.is_empty());
    assert_eq!(s.anyone().bytes("/api/index/sounds").await.0, 401);
    assert_eq!(ann.bytes(&format!("/api/index/sounds/changes?since={}", i64::MAX)).await.0, 410);

    // Fits and counts are each account's own: what ann did says nothing of bob.
    let marks = |bytes: Vec<u8>| cratarr_rules::index::Marks::load(&bytes).unwrap();
    let landing = of_ann.tracks.iter().find(|t| t.title == "Landing").unwrap().id;
    let song = format!("tr-{landing}");
    ann.ok(reqwest::Method::POST, &format!("/api/track/cratarr/{song}/favorite?on=true"), json!({})).await;
    ann.ok(reqwest::Method::POST, &format!("/api/track/cratarr/{song}/played"), json!({})).await;
    let (status, hers) = ann.bytes("/api/index/own").await;
    assert_eq!(status, 200);
    let (hers, his) = (marks(hers), marks(bob.bytes("/api/index/own").await.1));
    let mark = hers.of(landing).unwrap();
    assert_eq!(mark.plays, 1, "{mark:?}");
    assert!(mark.fit.is_some(), "a favourite makes a taste: {mark:?}");
    assert!(his.rows().is_empty(), "bob has done nothing: {:?}", his.rows());
    assert_eq!(s.anyone().bytes("/api/index/own").await.0, 401);

    // Small covers come as they are made: a record with a cover beside it or inside it, in the bundle sooner or later.
    let covers = |bytes: &[u8]| {
        assert_eq!(&bytes[..4], b"CRXV");
        let (mut at, mut records) = (16, std::collections::BTreeSet::new());
        while at < bytes.len() {
            let length = u32::from_le_bytes(bytes[at + 8..at + 12].try_into().unwrap()) as usize;
            assert!(length > 0, "a bundle holds what is there");
            records.insert(i64::from_le_bytes(bytes[at..at + 8].try_into().unwrap()));
            at += 12 + length;
        }
        records
    };
    let tide = of_ann.records.iter().find(|r| r.title == "Tide Tables").unwrap().id;
    let two_rooms = of_ann.records.iter().find(|r| r.title == "Two Rooms").unwrap().id;
    s.until("the small covers made", Duration::from_secs(120), || async { let held = covers(&ann.bytes("/api/index/covers").await.1); held.contains(&tide) && held.contains(&two_rooms) }).await;
    assert_eq!(s.anyone().bytes("/api/index/covers").await.0, 401);
    assert_eq!(ann.bytes(&format!("/api/index/covers/changes?since={}", i64::MAX)).await.0, 410);

    // Beside the user data's changes, a word on the library's log, by a rev of its own.
    let at = |rev: i64| format!("/api/changes?since=0&kinds=favourite,index&index={rev}");
    s.until("the library still", Duration::from_secs(60), || async {
        let rev = index_of(&ann).await.rev();
        ann.get(&at(rev)).await["index"] == json!({ "rev": rev, "changed": false })
    })
    .await;
    let told = ann.get(&at(i64::MAX)).await;
    assert!(told["index"]["reset"] == true && told["index"]["changed"] == true, "{told}");
    assert!(ann.get("/api/changes?since=0&kinds=favourite").await.get("index").is_none(), "only where it is asked for");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// What a device was told.
#[derive(Debug, PartialEq)]
enum Fed {
    Applied,
    Same,
    Again,
}

/// The feed from where `device` stands, applied.
async fn fed(who: &Caller<'_>, device: &mut cratarr_rules::index::Index) -> Fed {
    let r = who.request(reqwest::Method::GET, &format!("/api/index/changes?since={}", device.rev())).send().await.unwrap();
    match r.status().as_u16() {
        200 => {
            let bytes = r.bytes().await.unwrap();
            device.apply(&cratarr_rules::index::Changes::load(&bytes).unwrap()).unwrap();
            Fed::Applied
        }
        204 => Fed::Same,
        410 => Fed::Again,
        status => panic!("the feed answered {status}: {}", r.text().await.unwrap()),
    }
}

/// `device` on the feed until it holds what a fresh fetch holds, to the byte, and that is what the change said (`arrived`): a
/// change reaches the library a step at a time and the rows a step after, and until then a device and a fresh fetch agree on
/// what stood before. A feed that misses a row never gets there.
async fn in_step(who: &Caller<'_>, device: &mut cratarr_rules::index::Index, what: &str, arrived: impl Fn(&cratarr_rules::index::Index) -> bool) {
    let start = Instant::now();
    loop {
        assert_ne!(fed(who, device).await, Fed::Again, "{what}: told to start again");
        if arrived(device) && device.save() == index_of(who).await.save() {
            return;
        }
        assert!(start.elapsed() < Duration::from_secs(60), "{what}: the feed never came to what a fresh index says");
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// The owner's proof for the feed: a Keep, a retag on the disk, a delete and a fact set by hand each reach a device, which then
/// holds what a fresh fetch holds, and with nothing to tell it is told so.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_device_on_the_feed_holds_what_a_fresh_index_holds_end_to_end() {
    let (s, root, music, _) = on_music_and_incoming("feed").await;
    s.ok(reqwest::Method::POST, "/api/users", json!({ "name": ANN.0, "password": ANN.1, "admin": true })).await;
    let who = s.sign_in(ANN.0, ANN.1).await;
    let mut device = index_of(&who).await;
    let listed = |device: &cratarr_rules::index::Index, title: &str| device.rows().tracks.into_iter().find(|t| t.title == title);
    let id_of = |albums: Vec<Value>, title: &str| albums.iter().find(|a| a["title"] == title).unwrap()["id"].as_str().unwrap().to_string();

    let album = id_of(who.albums().await, "Just Arrived");
    let was = listed(&device, "Just Arrived").unwrap();
    who.ok(reqwest::Method::POST, &format!("/api/incoming/keep/cratarr/{album}"), json!({})).await;
    let in_music = music.to_string_lossy().to_string();
    s.until("the Keep listed", Duration::from_secs(60), || async { who.albums().await.iter().any(|a| a["title"] == "Just Arrived" && a["library"].as_str() == Some(&*in_music)) }).await;
    in_step(&who, &mut device, "a Keep", |d| listed(d, "Just Arrived").is_some_and(|t| t.facts.folders != was.facts.folders)).await;
    assert_eq!(listed(&device, "Just Arrived").unwrap().id, was.id, "the same track, in the library it was added to");

    let album = id_of(who.albums().await, "Tide Tables");
    set_vorbis(&music.join("Glass Harbour/Tide Tables (2012)/01 Slack Water.flac"), "TITLE", "Slacker Water");
    s.until("the retag read", Duration::from_secs(120), || async {
        ask(&who, "startScan", "").await;
        who.get(&format!("/api/albums/cratarr/{album}")).await["rows"].as_array().unwrap().iter().any(|r| r["name"] == "Slacker Water")
    })
    .await;
    in_step(&who, &mut device, "a retag", |d| listed(d, "Slacker Water").is_some() && listed(d, "Slack Water").is_none()).await;

    let album = id_of(who.albums().await, "Signal");
    let gone = listed(&device, "Signal").unwrap();
    who.ok(reqwest::Method::POST, &format!("/api/albums/cratarr/{album}/delete"), json!({})).await;
    s.until("the delete seen", Duration::from_secs(120), || async {
        ask(&who, "startScan", "").await;
        !who.albums().await.iter().any(|a| a["title"] == "Signal")
    })
    .await;
    in_step(&who, &mut device, "a delete", |d| d.track(gone.id).is_none() && d.record(gone.record).is_none()).await;

    let album = id_of(who.albums().await, "Undated");
    who.ok(reqwest::Method::PUT, "/api/albums/overrides", json!({ "albums": [album], "set": { "year": 1999 } })).await;
    s.until("the fact set", Duration::from_secs(90), || async { who.albums().await.iter().any(|a| a["title"] == "Undated" && a["year"] == 1999) }).await;
    in_step(&who, &mut device, "a fact set by hand, written to nothing the catalogue holds", |d| d.rows().records.iter().any(|r| r.title == "Undated" && r.year == Some(1999))).await;

    s.until("nothing more", Duration::from_secs(60), || async {
        let mut now = index_of(&who).await;
        fed(&who, &mut now).await == Fed::Same
    })
    .await;
    let r = who.request(reqwest::Method::GET, &format!("/api/index/changes?since={}", i64::MAX)).send().await.unwrap();
    assert_eq!(r.status().as_u16(), 410);
    let told = r.json::<Value>().await.unwrap();
    assert!(told["reset"] == true && told["rev"].as_i64().is_some_and(|rev| rev >= device.rev()), "start again, and from where: {told}");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// What a device keeps of an account (decision 0101, points 16 to 19): the index, its sounds and the account's own, as fetched.
struct DeviceKept {
    index: Vec<u8>,
    sounds: Vec<u8>,
    own: Vec<u8>,
}

impl DeviceKept {
    /// Everything a device fetches for `who`, whole.
    async fn fetched(who: &Caller<'_>) -> DeviceKept {
        let mut parts = Vec::new();
        for path in ["/api/index", "/api/index/sounds", "/api/index/own"] {
            let (status, bytes) = who.bytes(path).await;
            assert_eq!(status, 200, "{path}");
            parts.push(bytes);
        }
        let [index, sounds, own]: [Vec<u8>; 3] = parts.try_into().unwrap();
        DeviceKept { index, sounds, own }
    }

    /// The device answering from what it keeps, with no Cratarr to ask.
    fn device(&self) -> cratarr_rules::index::device::Device {
        let sounds = cratarr_rules::index::Sounds::load(&self.sounds).unwrap();
        let marks = cratarr_rules::index::Marks::load(&self.own).unwrap();
        cratarr_rules::index::device::Device::new(cratarr_rules::index::Index::load(&self.index).unwrap(), Some(&sounds), &marks)
    }

    /// What its search finds for `q`: the artists', records' and tracks' ids, in its order.
    fn found(&self, q: &str) -> [Vec<String>; 3] {
        let index = cratarr_rules::index::Index::load(&self.index).unwrap();
        let ask = cratarr_rules::index::find::Ask { q: q.to_string(), limit: 20, ..Default::default() };
        let found: Value = serde_json::from_str(&index.found_json(&index.finder(), &ask)).unwrap();
        let ids = |kind: &str| found[kind]["rows"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap().to_string()).collect();
        [ids("artists"), ids("records"), ids("tracks")]
    }
}

/// What `device` says to `asked`, which it must have an answer to.
fn told(device: &mut cratarr_rules::index::device::Device, asked: Value) -> Value {
    let answer = device.ask(&asked.to_string()).unwrap_or_else(|| panic!("no answer to {asked}"));
    serde_json::from_str(&answer).unwrap()
}

/// What the server's search finds for `q` in the library's tier, as a device finds it: the artists', records' and tracks' ids. What
/// it finds only through what one holds is the server's alone: an artist only by a record's name, a record only by a track's title.
/// A device finds the record, or the track (D17).
async fn found_by(who: &Caller<'_>, q: &str) -> [Vec<String>; 3] {
    let answer = who.get(&format!("/api/search?q={q}&kinds=artist,album,track&limit=20")).await;
    let items = |kind: &str| answer["groups"][kind]["items"].as_array().cloned().unwrap_or_default();
    let not_only = |item: &Value, through: &str| item["matched"].as_array().is_some_and(|m| m.iter().any(|m| m["on"] != through));
    let artists = items("artist").iter().filter(|i| not_only(i, "album")).map(|i| i["id"].as_str().unwrap().to_string()).collect();
    let records = items("album").iter().filter(|i| not_only(i, "track")).map(|i| i["subject"].as_str().unwrap().rsplit('/').next().unwrap().to_string()).collect();
    let tracks = items("track").iter().map(|i| i["track"].as_str().unwrap().to_string()).collect();
    [artists, records, tracks]
}

/// Found ids, each kind's in a fixed order. Results the words rank alike come in each one's own order: the server's full-text index
/// (bm25), a device's the index's rows. The same results, and where ranks tie the first may differ.
fn as_found(found: &[Vec<String>; 3]) -> [Vec<String>; 3] {
    found.clone().map(|mut ids| {
        ids.sort();
        ids
    })
}

/// The Crates page's list as each crate's id and counts.
fn listed_crates(list: &Value) -> Vec<(String, Value)> {
    list.as_array().unwrap().iter().map(|c| (c["id"].as_str().unwrap().to_string(), c["counts"].clone())).collect()
}

/// Rows' ids.
fn rows_of(answer: &Value) -> Vec<String> {
    answer["rows"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap().to_string()).collect()
}

/// The whole layout read, ann its administrator and bob, each of whom has hearted and played a track of their own and saved a crate
/// of what they hearted: the server, its folder, and each one's crate and track.
async fn two_listeners(name: &str) -> (Server, PathBuf, [(String, String); 2]) {
    let root = temp(name);
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    assert!(Command::new(BIN).arg("--make-layout").arg(&library).status().unwrap().success(), "the whole layout is made");
    settings(&data, &[(&library.join("music"), "main"), (&library.join("second"), "main"), (&library.join("incoming"), "incoming")], &library.join("own"));
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.ok(reqwest::Method::POST, "/api/users", json!({ "name": "ann", "password": "ann's password", "admin": true })).await;
    let mut chose = Vec::new();
    {
        let ann = s.sign_in("ann", "ann's password").await;
        ann.ok(reqwest::Method::POST, "/api/users", json!({ "name": "bob", "password": "bob's password" })).await;
        let titles = |albums: Vec<Value>| albums.iter().filter_map(|a| a["title"].as_str().map(str::to_string)).collect::<std::collections::BTreeSet<String>>();
        s.until("every library read", Duration::from_secs(180), || async { titles(ann.albums().await).len() == 22 }).await;
        let bob = s.sign_in("bob", "bob's password").await;
        let tracks = index_of(&ann).await.rows().tracks;
        for (who, title) in [(&ann, "Landing"), (&bob, "Other Room")] {
            let song = format!("tr-{}", tracks.iter().find(|t| t.title == title).unwrap().id);
            who.ok(reqwest::Method::POST, &format!("/api/track/cratarr/{song}/favorite?on=true"), json!({})).await;
            who.ok(reqwest::Method::POST, &format!("/api/track/cratarr/{song}/played"), json!({})).await;
            let conditions = json!([{ "axis": "favourite", "not": false, "value": { "kind": "yes" } }]);
            let made = who.ok(reqwest::Method::POST, "/api/crates", json!({ "name": "Hearted", "conditions": conditions })).await;
            chose.push((made["id"].as_str().unwrap().to_string(), song));
        }
    }
    let [ann, bob]: [(String, String); 2] = chose.try_into().unwrap();
    (s, root, [ann, bob])
}

/// D21, the network cut (decision 0101 point 28): two accounts' devices fetch what they keep, then Cratarr is gone, and each device
/// finds what the server's search found in the library's tier, lists the Crates page and the account's own crate as the server did,
/// and picks Radio's next among what plays on it. Each answers by its own account's marks, not the other's.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn cut_off_a_device_finds_lists_crates_and_picks_among_what_plays_here_as_the_server_did_end_to_end() {
    let (s, root, chose) = two_listeners("device-cut").await;
    let words = ["harbour", "room", "shores", "tide"];
    let mut online = Vec::new();
    for (name, (crate_id, _)) in [("ann", &chose[0]), ("bob", &chose[1])] {
        let who = s.sign_in(name, &format!("{name}'s password")).await;
        let kept = DeviceKept::fetched(&who).await;
        let mut found = Vec::new();
        for q in words {
            found.push(found_by(&who, q).await);
        }
        let (list, rows) = (who.get("/api/crates").await, who.get(&format!("/api/crates/{crate_id}/rows?offset=0&limit=200")).await);
        online.push((kept, found, listed_crates(&list), rows_of(&rows)));
    }
    s.stop();

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    let mut hearted = Vec::new();
    for ((kept, found, list, rows), (crate_id, song)) in online.iter().zip(&chose) {
        for (q, server) in words.iter().zip(found) {
            assert_eq!(as_found(&kept.found(q)), as_found(server), "{song}'s device searching {q:?}");
        }
        let mut device = kept.device();
        let page = listed_crates(&told(&mut device, json!({ "ask": "crates", "without": [], "now": now })));
        let ids = |l: &[(String, Value)]| l.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&page), ids(list), "the Crates page");
        let mut counted = 0;
        for ((id, here), (_, there)) in page.iter().zip(list) {
            for (toggle, n) in here.as_object().unwrap() {
                // A Sounds like's sizes at the toggles not set are the server's second and third pass: a device leaves them out (P6).
                if !n.is_null() {
                    assert_eq!(n, &there[toggle], "{id}'s count, {toggle}");
                    counted += 1;
                }
            }
        }
        assert!(counted > page.len(), "the counts compared: {counted} for {} crates", page.len());
        let own = rows_of(&told(&mut device, json!({ "ask": "rows", "id": crate_id, "offset": 0, "limit": 200, "now": now })));
        assert_eq!(&own, rows, "the crate {crate_id}");
        assert_eq!(own, [song.clone()], "what this account hearted, and only that");
        hearted.push(own);

        // What plays here: Both Shores and the hearted track, the one playing.
        let tracks = cratarr_rules::index::Index::load(&kept.index).unwrap().rows().tracks;
        let shores = tracks.iter().find(|t| t.title == "Landing").unwrap().record;
        let here: Vec<String> = tracks.iter().filter(|t| t.record == shores || format!("tr-{}", t.id) == *song).map(|t| format!("tr-{}", t.id)).collect();
        let asked = json!({ "ask": "pick", "mode": "random", "seed": [song], "exclude": [song], "last": song, "count": 5, "here": here, "draw": 7, "now": now });
        let picked: Vec<String> = told(&mut device, asked)["tracks"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap().to_string()).collect();
        let others: Vec<&String> = here.iter().filter(|id| *id != song).collect();
        assert!(!picked.is_empty() && picked.iter().all(|id| others.contains(&id)), "{song}: picked {picked:?} of {others:?}");

        // Radio by this account's marks, never another's (P7-REVIEW): one ask on each device, the same seed and draw, favourites left
        // out, among both accounts' hearts and Both Shores. Each leaves out its own heart, and takes the other's.
        let other = &chose.iter().find(|(_, s)| s != song).unwrap().1;
        let crossing = format!("tr-{}", tracks.iter().find(|t| t.title == "Crossing").unwrap().id);
        let here: Vec<String> = tracks.iter().filter(|t| t.record == shores).map(|t| format!("tr-{}", t.id)).chain([song.clone(), other.clone()]).collect();
        let asked = json!({ "ask": "pick", "mode": "random", "seed": [crossing], "exclude": [crossing], "last": crossing, "count": 10, "here": here, "skip_favourites": true, "draw": 7, "now": now });
        let picked: Vec<String> = told(&mut device, asked)["tracks"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap().to_string()).collect();
        assert!(!picked.contains(song) && picked.contains(other), "{song}'s device, favourites left out: picked {picked:?}");
    }
    assert_ne!(hearted[0], hearted[1], "each device by its own account's marks");
    std::fs::remove_dir_all(&root).ok();
}

/// D21, hidden but kept (decision 0101 point 28): a device keeps an account's index through its sign-out, and at the account's next
/// sign-in goes on from it by the feed, nothing fetched whole, while another account on the same device changed the library in
/// between and has his own marks.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn signed_out_and_in_again_a_device_goes_on_from_what_it_kept_end_to_end() {
    let (s, root, chose) = two_listeners("device-sign-out").await;
    // A favourite makes a taste, and a taste marks every track: what tells one account's from another's is what it played.
    let plays = |marks: &cratarr_rules::index::Marks, song: &str| marks.of(song.trim_start_matches("tr-").parse().unwrap()).map_or(0, |m| m.plays);
    let ann = s.sign_in("ann", "ann's password").await;
    let mut device = index_of(&ann).await;
    let hers = cratarr_rules::index::Marks::load(&ann.bytes("/api/index/own").await.1).unwrap();
    // Signed out as the page's form does, the sign-in page not followed: it would go on to the page itself.
    let form = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().unwrap();
    let r = form.post(format!("{}/signout", s.base)).header(reqwest::header::COOKIE, ann.session.clone().unwrap()).send().await.unwrap();
    assert_eq!(r.status().as_u16(), 303, "sent to the sign-in page");
    assert_eq!(ann.bytes("/api/index").await.0, 401, "her session is over");

    // Another account on the same device: his own marks, and a fact of his own record set by hand meanwhile.
    let bob = s.sign_in("bob", "bob's password").await;
    let his = cratarr_rules::index::Marks::load(&bob.bytes("/api/index/own").await.1).unwrap();
    assert_eq!((plays(&his, &chose[1].1), plays(&his, &chose[0].1)), (1, 0), "his play, none of hers");
    let room = bob.albums().await.into_iter().find(|a| a["title"] == "Other Room").unwrap()["id"].as_str().unwrap().to_string();
    bob.ok(reqwest::Method::PUT, "/api/albums/overrides", json!({ "albums": [room], "set": { "year": 1999 } })).await;
    s.until("the fact set", Duration::from_secs(90), || async { bob.albums().await.iter().any(|a| a["title"] == "Other Room" && a["year"] == 1999) }).await;

    // Her next sign-in: the feed goes on from her device's rev, to what a fresh fetch holds.
    let ann = s.sign_in("ann", "ann's password").await;
    in_step(&ann, &mut device, "a fact set while she was away", |d| d.rows().records.iter().any(|r| r.title == "Other Room" && r.year == Some(1999))).await;
    let again = cratarr_rules::index::Marks::load(&ann.bytes("/api/index/own").await.1).unwrap();
    assert_eq!((plays(&hers, &chose[0].1), plays(&again, &chose[0].1), plays(&again, &chose[1].1)), (1, 1, 0), "her play as she left it, none of his");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// A slskd for a fetch to meet: every batch taken and complete at once, `file` in the folder its batch names under
/// `completed`. Its address, and how many batches it was asked for.
async fn slskd_serving(completed: PathBuf, file: PathBuf) -> (String, Arc<AtomicUsize>) {
    use axum::routing::{get, post};
    let batches = Arc::new(AtomicUsize::new(0));
    let asked = batches.clone();
    let take = move |axum::Json(b): axum::Json<Value>| {
        let (completed, file, asked) = (completed.clone(), file.clone(), asked.clone());
        async move {
            asked.fetch_add(1, Ordering::SeqCst);
            let dir = completed.join("cratarr").join(b["id"].as_str().unwrap());
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::copy(&file, dir.join(file.file_name().unwrap())).unwrap();
            axum::Json(json!({ "batch": { "transfers": [] } }))
        }
    };
    let done = || async { axum::Json(json!({ "transfers": [{ "id": "t1", "filename": "x", "state": "Completed, Succeeded", "percentComplete": 100.0 }] })) };
    let app = axum::Router::new().route("/api/v0/transfers/downloads/batches", post(take)).route("/api/v0/transfers/downloads/batches/{id}", get(done));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, batches)
}

/// Soulseek's queue as a next start reads it: one record approved, of one track, in two users' folders.
fn soulseek_queue(data: &Path, file: &str) {
    let folder = |user: &str| json!({ "user": user, "dir": "d", "format": "flac", "lossless": true, "free_slot": true, "queue": 0, "speed": 1, "extra": 0, "files": [{ "filename": format!("d\\{file}"), "size": 1 }] });
    let record = json!({ "artist": "Glass Harbour", "album": "Tide Tables", "release_group": "rg", "request": 1 });
    let item = json!({ "id": "ss-1", "record": record, "state": "approved", "tracks": [{ "disc": 1, "position": 1, "title": "Slack Water", "recording": "r1" }], "folders": [folder("ann"), folder("bo")], "chosen": 0 });
    std::fs::write(data.join("soulseek.json"), serde_json::to_vec(&json!({ "next": 1, "items": [item] })).unwrap()).unwrap();
}

/// A service away is no bad file and no bad user (audit findings 11 and 12), on the real worker through a slskd's HTTP: with
/// no AcoustID key the record waits at its check with both folders, across a restart, fetching nothing by itself; it can
/// leave the list, and what it fetched with it. With slskd gone it stays approved and blames no one.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_record_whose_services_are_away_waits_and_blames_no_user_end_to_end() {
    let root = temp("soulseek");
    let (library, data, completed) = (root.join("library"), root.join("data"), root.join("slskd"));
    std::fs::create_dir_all(&data).unwrap();
    make_layout(&library);
    settings(&data, &[(&library.join("music"), "main"), (&library.join("incoming"), "incoming")], &library.join("own"));
    let file = library.join("music/Glass Harbour/Tide Tables (2012)/01 Slack Water.flac");
    let name = file.file_name().unwrap().to_string_lossy().to_string();
    let (url, batches) = slskd_serving(completed.clone(), file.clone()).await;
    let record = |s: &Server| {
        let http = (s.http.clone(), format!("{}/api/soulseek", s.base));
        async move { http.0.get(http.1).send().await.unwrap().json::<Value>().await.unwrap()["items"][0].clone() }
    };

    // slskd first: the queue is read once, as the server starts.
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/slskd", json!({ "url": url, "key": "k", "folder": completed, "enabled": true })).await;
    s.stop();
    soulseek_queue(&data, &name);
    let s = Server::start(&data).await;
    s.until("the fetch to reach its check", Duration::from_secs(60), || async { record(&s).await["why"].is_string() }).await;
    let i = record(&s).await;
    assert_eq!((i["state"].as_str(), i["why"].as_str()), (Some("checking"), Some("AcoustID is not set up: Settings › Server")), "{i}");
    assert_eq!((i["folders"].as_array().unwrap().len(), &i["failed"], &i["set_aside"]), (2, &json!([]), &json!([])), "no user held to it, nothing set aside");
    let fetched = completed.join("cratarr").join(i["batch"].as_str().unwrap());
    assert!(fetched.join(&name).exists(), "what it fetched waits where it is");

    // Across a restart: still held, and nothing fetched by itself.
    s.stop();
    let s = Server::start(&data).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(record(&s).await["state"], "checking");
    assert_eq!(batches.load(Ordering::SeqCst), 1, "no next folder under the same tick");

    // Out of the list, what it fetched with it: nothing else looks in a batch's folder.
    s.ok(reqwest::Method::DELETE, "/api/soulseek/ss-1", json!({})).await;
    assert_eq!(record(&s).await, Value::Null);
    assert!(!fetched.exists());

    // slskd gone: nothing listens there.
    let gone = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    s.ok(reqwest::Method::PUT, "/api/settings/slskd", json!({ "url": format!("http://{gone}") })).await;
    s.stop();
    soulseek_queue(&data, &name);
    let s = Server::start(&data).await;
    s.until("the fetch to find slskd gone", Duration::from_secs(60), || async { record(&s).await["why"].is_string() }).await;
    let i = record(&s).await;
    assert!(i["why"].as_str().unwrap().starts_with("slskd could not be reached"), "{i}");
    assert_eq!((i["state"].as_str(), i["chosen"].as_i64(), i["folders"].as_array().unwrap().len(), &i["failed"]), (Some("approved"), Some(0), 2, &json!([])), "its tick and folders kept, no user failed");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn types_and_years_into_the_files_end_to_end() {
    let root = temp("types");
    let (music, data) = (root.join("music"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_fixtures(&music);
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

/// The types of every FLAC under `dir`, by its path. A hidden file is a copy a stop left, no track.
fn types_under(dir: &Path) -> std::collections::BTreeMap<String, Vec<String>> {
    let tracks = audio_under(dir).into_iter().filter(|f| f.ends_with(".flac") && !f.rsplit('/').next().unwrap().starts_with('.'));
    tracks.map(|f| (f.clone(), vorbis(&dir.join(&f), "RELEASETYPE"))).collect()
}

/// A run's records as its page says them: each record and how many of its files were written.
fn written(run: &Value) -> Vec<(String, u64)> {
    run["records"].as_array().unwrap().iter().map(|r| (r["album"].as_str().unwrap().to_string(), r["written"].as_u64().unwrap())).collect()
}

/// A real stop: the server killed while a run writes, not a state made to look like one. The run is listed as cut off, read at
/// start, and keeps to what it wrote: the write tried again for what is left is the second run's, on the first's page and in
/// each one's Undo.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_run_killed_half_way_keeps_to_what_it_wrote_end_to_end() {
    let root = temp("killed");
    let (music, data) = (root.join("music"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_fixtures(&music);
    settings_0_15(&data, &music.to_string_lossy());

    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.until("the fixture library read", Duration::from_secs(120), || async { s.albums().await.len() == 12 }).await;
    s.ok(reqwest::Method::PUT, "/api/settings/file-facts", json!({ "on": true })).await;
    let first = types_under(&music);

    s.write_the_sheet().await;
    // Albums go at least 50 ms apart, so asked this often the run is caught with most of them to go.
    let asked = Instant::now();
    loop {
        let runs = s.get("/api/file-changes/runs").await;
        assert_eq!(runs[0]["id"].as_str().unwrap().chars().next(), Some('l'), "it finished before it could be stopped: {runs}");
        if runs[0]["done"].as_u64().unwrap() >= 2 {
            break;
        }
        assert!(asked.elapsed() < Duration::from_secs(60), "waited for the run to get going: {runs}");
    }
    s.stop();

    let s = Server::start(&data).await;
    let cut = s.get("/api/file-changes/runs").await[0].clone();
    let id = cut["id"].as_str().unwrap().to_string();
    assert!(cut["failed"].as_array().unwrap().iter().any(|f| f["why"] == "Cratarr stopped before this was done"), "{cut}");
    let page = format!("/api/file-changes/runs/{id}");
    let stopped = types_under(&music);
    let wrote: Vec<&String> = stopped.keys().filter(|f| stopped[*f] != first[*f]).collect();
    assert!(!wrote.is_empty(), "it wrote before it was stopped");
    // Nothing has written since: the files alone hold it to what it reached, a record's worth of them said on its list.
    let reached = written(&s.get(&page).await).iter().filter(|(_, n)| *n > 0).count() as u64;
    s.until("the run cut off read at start", Duration::from_secs(60), || async { s.get("/api/file-changes/runs").await[0]["albums"] == reached }).await;
    let claimed = written(&s.get(&page).await);

    s.write_the_sheet().await;
    let second = s.done_run(Some(&id)).await["id"].as_str().unwrap().to_string();
    let both = types_under(&music);
    assert_ne!(both, stopped, "the second run wrote what the first did not reach");
    assert_eq!(written(&s.get(&page).await), claimed, "the first run claims nothing the second wrote");

    let (status, undone) = s.undo(&id).await;
    assert_eq!((status, &undone["undone"]), (200, &json!(true)), "{undone}");
    for (f, types) in types_under(&music) {
        assert_eq!(types, if wrote.contains(&&f) { first[&f].clone() } else { both[&f].clone() }, "{f}");
    }
    let (status, undone) = s.undo(&second).await;
    assert_eq!((status, &undone["undone"]), (200, &json!(true)), "{undone}");
    assert_eq!(types_under(&music), first, "every file as it was before either run");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// A real stop in a file written whole: a cover killed with its new picture beside the tracks and its tracks under way. Nothing
/// but the run's entry knows what the folder held.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_cover_killed_part_way_is_put_back_by_its_undo_end_to_end() {
    let root = temp("cover");
    let (music, data) = (root.join("music"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_fixtures(&music);
    settings_0_15(&data, &music.to_string_lossy());

    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.until("the fixture library read", Duration::from_secs(120), || async { s.albums().await.len() == 12 }).await;
    let album = s.albums().await.into_iter().find(|a| a["title"] == "Club Classics").expect("eight tracks to be part way through");
    let dir = music.join("Various Artists/Club Classics (2001)");
    let cover = dir.join("cover.jpg");
    let (old, new) = (std::fs::read(&cover).unwrap(), std::fs::read(music.join("Glass Harbour/Tide Tables (2012)/cover.jpg")).unwrap());
    assert_ne!(old, new);

    let url = format!("{}/api/albums/cratarr/{}/covers", s.base, album["id"].as_str().unwrap());
    let sent = s.http.put(url).body(new.clone()).send().await.unwrap();
    assert!(sent.status().is_success(), "{}", sent.status());
    let asked = Instant::now();
    while std::fs::read(&cover).ok().as_ref() != Some(&new) {
        assert!(asked.elapsed() < Duration::from_secs(60), "waited for the new cover");
    }
    s.stop();

    let s = Server::start(&data).await;
    let run = s.get("/api/file-changes/runs").await[0].clone();
    assert_eq!(run["what"], "cover", "{run}");
    assert!(run["failed"].as_array().unwrap().iter().any(|f| f["why"] == "Cratarr stopped before this was done"), "stopped part way: {run}");
    let (status, undone) = s.undo(run["id"].as_str().unwrap()).await;
    assert_eq!((status, &undone["undone"]), (200, &json!(true)), "{undone}");
    assert_eq!(std::fs::read(&cover).unwrap(), old, "the old picture back");
    for track in audio_under(&dir).iter().filter(|f| !f.starts_with('.')) {
        use lofty::prelude::*;
        let file = lofty::read_from_path(dir.join(track)).unwrap();
        assert_eq!(file.tags().iter().map(|t| t.picture_count()).sum::<u32>(), 0, "{track} had no picture inside");
    }
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// The files of a folder as they lie, by their place in it.
type Files = std::collections::BTreeMap<String, Vec<u8>>;

/// Every file under `dir`. What is hidden is Cratarr's own: its trash, a copy a stop left.
fn as_it_lies(dir: &Path) -> Files {
    fn walk(root: &Path, at: &Path, out: &mut Files) {
        for e in std::fs::read_dir(at).unwrap().flatten() {
            let p = e.path();
            if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                out.insert(p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"), std::fs::read(&p).unwrap());
            }
        }
    }
    let mut out = Files::new();
    walk(dir, dir, &mut out);
    out
}

/// What files hold, whatever they are called: Keep and Replace name them by the library's template.
fn contents(files: &Files) -> Vec<&Vec<u8>> {
    let mut all: Vec<&Vec<u8>> = files.values().collect();
    all.sort();
    all
}

/// The audio in a library's trash.
fn in_the_trash(library: &Path) -> usize {
    let trash = library.join(".cratarr-trash");
    if trash.is_dir() { audio_under(&trash).len() } else { 0 }
}

/// The pictures a track holds inside, each as it lies.
fn pictures_inside(track: &Path) -> Vec<Vec<u8>> {
    use lofty::prelude::*;
    lofty::read_from_path(track).unwrap().tags().iter().flat_map(|t| t.pictures()).map(|p| p.data().to_vec()).collect()
}

/// A track's length as its file's header says it, in ms.
fn header_ms(track: &Path) -> u128 {
    use lofty::prelude::*;
    lofty::read_from_path(track).unwrap().properties().duration().as_millis()
}

/// A server on the fixture library, read, nothing asked from outside: it, where it all lies, the library and the data folder.
async fn on_the_fixtures(name: &str) -> (Server, PathBuf, PathBuf, PathBuf) {
    let root = temp(name);
    let (music, data) = (root.join("music"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_fixtures(&music);
    settings_0_15(&data, &music.to_string_lossy());
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.until("the fixture library read", Duration::from_secs(120), || async { s.albums().await.len() == 12 }).await;
    (s, root, music, data)
}

/// A server on the whole layout's Music and Incoming, both read: it, where it all lies, and the two folders.
async fn on_music_and_incoming(name: &str) -> (Server, PathBuf, PathBuf, PathBuf) {
    let root = temp(name);
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_layout(&library);
    let (music, incoming) = (library.join("music"), library.join("incoming"));
    settings(&data, &[(&music, "main"), (&incoming, "incoming")], &library.join("own"));
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.read_in("Tide Tables", &music).await;
    s.read_in("Just Arrived", &incoming).await;
    (s, root, music, incoming)
}

/// A copy's track ids, by where its files lie: a record's page lists its tracks, and only the catalogue can say a copy's.
fn tracks_at(data: &Path, library: &Path, dir: &str) -> Vec<String> {
    let db = rusqlite::Connection::open(data.join("catalogue.sqlite")).unwrap();
    db.busy_timeout(Duration::from_secs(10)).unwrap();
    let mut st = db.prepare("SELECT 'tr-' || f.track FROM files f JOIN folders d ON d.id = f.folder WHERE d.path = ?1 AND f.rel LIKE ?2 AND f.track IS NOT NULL ORDER BY f.rel").unwrap();
    let rows = st.query_map([library.to_string_lossy().to_string(), format!("{dir}/%")], |r| r.get::<_, String>(0)).unwrap();
    rows.map(Result::unwrap).collect()
}

/// Keep: the album leaves Incoming for Music with its track ids and every byte, and is on the list of runs. Its Undo puts each
/// file back where it was, under its old name, and Music holds nothing of it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn keep_moves_an_album_with_its_ids_and_its_undo_puts_it_back_end_to_end() {
    let (s, root, music, incoming) = on_music_and_incoming("keep").await;
    let album = s.incoming_album("Just Arrived").await;
    let path = format!("/api/incoming/keep/cratarr/{}", album["id"].as_str().unwrap());
    let arrived = incoming.join("Salt Meridian/Just Arrived (2018)");
    let (files, songs) = (as_it_lies(&arrived), s.songs_of(&album).await);
    assert_eq!(files.len(), 1, "{:?}", files.keys());

    let preview = s.get(&path).await;
    assert_eq!((preview["files"].as_u64(), &preview["refused"]), (Some(1), &Value::Null), "{preview}");
    let kept = s.ok(reqwest::Method::POST, &path, json!({})).await;
    assert_eq!((&kept["queued"], &kept["folder"]), (&json!(false), &preview["folder"]), "{kept}");
    let folder = music.join(kept["folder"].as_str().unwrap());
    assert!(contents(&as_it_lies(&folder)) == contents(&files), "not the same files in Music");
    assert!(!arrived.exists(), "still in Incoming");

    let in_music = s.read_in("Just Arrived", &music).await;
    assert_eq!(s.songs_of(&in_music).await, songs, "its track ids kept");
    assert!(contents(&files) == [&s.served(&songs[0]).await], "not served from where it went");

    let run = s.album_run("keep").await;
    let id = run["id"].as_str().unwrap();
    let (status, undone) = s.undo_a_moment_later(id).await;
    assert_eq!((status, &undone["undone"]), (200, &json!(true)), "{undone}");
    assert!(as_it_lies(&arrived) == files, "not back as it was");
    assert!(!folder.exists(), "still in Music");
    let back = s.read_in("Just Arrived", &incoming).await;
    assert_eq!(s.songs_of(&back).await, songs, "its track ids back with it");
    s.until("Music to let it go", Duration::from_secs(120), || async { s.album_in("Just Arrived", &music).await.is_none() }).await;
    assert_eq!(s.send(reqwest::Method::POST, &format!("/api/file-changes/runs/{id}/undo"), json!({})).await.0, 409, "undone once");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// A real bug, not in the gate: its name has no end_to_end in it, which is what the gate runs. `Serve::refresh` knows the catalogue
/// changed by `last_pass`, in whole seconds: an Undo within the second of its last look moves the rows back unseen, and if the pass
/// that follows ends in that second too, the library lists the album in Music until the next pass, five minutes on.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "a real bug: an Undo within a second of the library's last look stays unseen until the next pass (Serve::refresh's last_pass)"]
async fn an_undo_in_the_second_of_its_keep_is_seen_before_the_next_pass() {
    let (s, root, music, incoming) = on_music_and_incoming("keep-at-once").await;
    let album = s.incoming_album("Just Arrived").await;
    s.ok(reqwest::Method::POST, &format!("/api/incoming/keep/cratarr/{}", album["id"].as_str().unwrap()), json!({})).await;
    s.read_in("Just Arrived", &music).await;
    let run = s.album_run("keep").await;
    s.undo(run["id"].as_str().unwrap()).await;
    s.read_in("Just Arrived", &incoming).await;
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// Delete, from Incoming and from Music: into that library's own trash and off the list. The one Undo brings back every file and
/// the same track ids, once.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn delete_goes_to_its_librarys_trash_and_its_undo_brings_it_back_end_to_end() {
    let (s, root, music, incoming) = on_music_and_incoming("delete").await;
    let arriving = s.incoming_album("Just Arrived").await;
    let held = s.read_in("Tide Tables", &music).await;
    let cases = [
        (&incoming, "Just Arrived", "Salt Meridian/Just Arrived (2018)", format!("/api/incoming/delete/cratarr/{}", arriving["id"].as_str().unwrap()), &arriving),
        (&music, "Tide Tables", "Glass Harbour/Tide Tables (2012)", format!("/api/albums/cratarr/{}/delete", held["id"].as_str().unwrap()), &held),
    ];
    for (library, title, dir, path, album) in cases {
        let dir = library.join(dir);
        let (files, songs) = (as_it_lies(&dir), s.songs_of(album).await);
        let audio = audio_under(&dir).len();
        let gone = s.ok(reqwest::Method::POST, &path, json!({})).await;
        assert_eq!(gone["title"], title, "{gone}");
        assert!(!dir.exists(), "{title} still there");
        assert_eq!(in_the_trash(library), audio, "{title} in its own library's trash");
        s.until("the library to let it go", Duration::from_secs(120), || async { s.album_in(title, library).await.is_none() }).await;

        let tokens = json!({ "tokens": [gone["undo"]] });
        let undone = s.ok(reqwest::Method::POST, "/api/undo", tokens.clone()).await;
        assert_eq!(undone["restored"], json!([title]), "{undone}");
        assert!(as_it_lies(&dir) == files, "{title} not back as it was");
        assert_eq!(in_the_trash(library), 0, "{title} out of the trash");
        let back = s.read_in(title, library).await;
        assert_eq!(s.songs_of(&back).await, songs, "{title}'s track ids");
        assert_eq!(s.send(reqwest::Method::POST, "/api/undo", tokens).await.0, 409, "{title} undone once");
    }
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// A record in two libraries is one entry, so what acts on it must name a copy. Delete and Move land on the copy named and on no
/// other, whichever plays; a record of several copies with none named is refused, and nothing moves. Each picture is its copy's.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn the_copy_named_is_the_copy_touched_end_to_end() {
    let root = temp("named");
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_layout(&library);
    let (music, second, incoming) = (library.join("music"), library.join("second"), library.join("incoming"));
    let dir = "Salt Meridian/Both Shores (2018)";
    // The second main library without its copy, for one to move to.
    std::fs::remove_dir_all(second.join(dir)).unwrap();
    for (folder, picture) in [(&music, b"\xff\xd8the kept copy's".as_slice()), (&incoming, b"\xff\xd8the waiting copy's".as_slice())] {
        std::fs::write(folder.join(dir).join("cover.jpg"), picture).unwrap();
    }
    settings(&data, &[(&music, "main"), (&second, "main"), (&incoming, "incoming")], &library.join("own"));
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    let path = |p: &Path| p.to_string_lossy().to_string();
    s.read_in("Both Shores", &music).await;
    let album = s.read_in("Both Shores", &incoming).await;
    let id = album["id"].as_str().unwrap();
    assert_eq!(album["library"].as_str(), Some(&*path(&music)), "the entry is the copy that plays");
    let (kept, waiting) = (as_it_lies(&music.join(dir)), as_it_lies(&incoming.join(dir)));
    let (deleting, moving) = (format!("/api/albums/cratarr/{id}/delete"), format!("/api/albums/cratarr/{id}/move"));

    let picture = |library: &Path| format!("/api/albums/cratarr/{id}/cover?from={}", path(library).replace('/', "%2F"));
    assert!(s.anyone().bytes(&picture(&music)).await.1.ends_with(b"the kept copy's"));
    assert!(s.anyone().bytes(&picture(&incoming)).await.1.ends_with(b"the waiting copy's"));

    assert_eq!(s.send(reqwest::Method::POST, &deleting, json!({})).await.0, 409, "two copies, none named");
    assert_eq!(s.send(reqwest::Method::POST, &deleting, json!({ "from": path(&second) })).await.0, 409, "no copy there");
    assert_eq!(s.send(reqwest::Method::POST, &moving, json!({ "to": path(&second) })).await.0, 409, "two copies, none named");
    assert_eq!(s.send(reqwest::Method::POST, &moving, json!({ "from": path(&music), "to": path(&music) })).await.0, 409, "it is in that library already, and Incoming's folder of that name is not it");
    assert!(as_it_lies(&music.join(dir)) == kept && as_it_lies(&incoming.join(dir)) == waiting, "a refusal moves nothing");

    // The copy that does not play: the record stays, and the one that plays stays with it.
    let gone = s.ok(reqwest::Method::POST, &deleting, json!({ "from": path(&incoming) })).await;
    assert!(!incoming.join(dir).exists(), "the copy named went");
    assert!(as_it_lies(&music.join(dir)) == kept, "the other copy stands");
    s.until("Incoming to let its copy go", Duration::from_secs(120), || async { s.album_in("Both Shores", &incoming).await.is_none() }).await;
    s.read_in("Both Shores", &music).await;
    s.ok(reqwest::Method::POST, "/api/undo", json!({ "tokens": [gone["undo"]] })).await;
    assert!(as_it_lies(&incoming.join(dir)) == waiting);
    s.read_in("Both Shores", &incoming).await;

    // The copy that plays: named, it goes, and Incoming's is left.
    let gone = s.ok(reqwest::Method::POST, &deleting, json!({ "from": path(&music) })).await;
    assert!(!music.join(dir).exists(), "the copy named went");
    assert!(as_it_lies(&incoming.join(dir)) == waiting, "the other copy stands");
    s.until("Music to let its copy go", Duration::from_secs(120), || async { s.album_in("Both Shores", &music).await.is_none() }).await;
    s.ok(reqwest::Method::POST, "/api/undo", json!({ "tokens": [gone["undo"]] })).await;
    s.read_in("Both Shores", &music).await;

    // Moved out of Incoming, Music's stands.
    let asked = format!("{moving}?from={}&to={}", path(&incoming).replace('/', "%2F"), path(&second).replace('/', "%2F"));
    assert_eq!(s.get(&asked).await["refused"], Value::Null);
    let moved = s.ok(reqwest::Method::POST, &moving, json!({ "from": path(&incoming), "to": path(&second) })).await;
    assert!(!incoming.join(dir).exists(), "out of Incoming");
    // Its cover goes into its tracks on the way in, so by what they are: Incoming's MP3s, not Music's FLACs.
    let arrived = audio_under(&second.join(moved["folder"].as_str().unwrap()));
    assert!(arrived.len() == 3 && arrived.iter().all(|f| f.ends_with(".mp3")), "into the library named, the copy named: {arrived:?}");
    assert!(as_it_lies(&music.join(dir)) == kept, "the other copy stands");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// The intake card is about the copy waiting, though the record plays from Music: its tracks are those files, what is heard is
/// heard of them and kept under that copy, and a snippet streams from it. A plan for it writes to it alone.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn the_card_hears_the_incoming_copy_end_to_end() {
    let root = temp("card");
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_layout(&library);
    let (music, incoming) = (library.join("music"), library.join("incoming"));
    // No own music: two copies, the record's and the one waiting.
    settings(&data, &[(&music, "main"), (&incoming, "incoming")], &root.join("no-own-music"));
    // A rule, so a plan has something to write in any copy of the record.
    let mut saved: Value = serde_json::from_slice(&std::fs::read(data.join("settings.json")).unwrap()).unwrap();
    saved["tag_rules"] = json!({ "genre_aliases": [{ "from": ["Electronic"], "to": "Electronica" }] });
    std::fs::write(data.join("settings.json"), serde_json::to_vec_pretty(&saved).unwrap()).unwrap();
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.ok(reqwest::Method::POST, "/api/users", json!({ "name": "ann", "password": "ann's password", "admin": true })).await;
    let ann = s.sign_in("ann", "ann's password").await;
    let dir = "Salt Meridian/Both Shores (2018)";
    let both = |a: &Value| a["title"] == "Both Shores" && a["copies"].as_array().is_some_and(|c| c.len() == 2);
    s.until("both copies read", Duration::from_secs(120), || async { ann.albums().await.iter().any(both) }).await;
    let album = ann.albums().await.into_iter().find(|a| both(a)).unwrap();
    let id = album["id"].as_str().unwrap();
    assert_eq!(album["format"], "FLAC", "the record plays from Music");
    let waiting = tracks_at(&data, &incoming, dir);
    let from = incoming.to_string_lossy().replace('/', "%2F");

    assert_eq!(waiting, tracks_at(&data, &music, dir), "the copy took the record's tracks");
    let card = ann.get(&format!("/api/albums/cratarr/{id}/card?from={from}")).await;
    assert!(card["format"].as_str().unwrap().starts_with("MP3"), "{card}");
    assert_eq!(card["library"].as_str(), Some(&*incoming.to_string_lossy()));
    let listed: Vec<String> = card["tracks"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap().to_string()).collect();
    assert_eq!(listed, waiting, "its tracks are the copy's files");

    // Heard once, in the background, by its own files' names.
    s.until("the copy heard", Duration::from_secs(120), || async {
        let card = ann.get(&format!("/api/albums/cratarr/{id}/card?from={from}")).await;
        card["checks"].as_array().unwrap().iter().any(|c| c["key"] == "sound" && c["state"] != "waiting")
    })
    .await;
    let user = rusqlite::Connection::open(data.join("userdata.sqlite")).unwrap();
    let (folder, heard): (i64, String) = user.query_row("SELECT folder, heard FROM intake_heard WHERE album = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert!(folder >= 0 && heard.contains(".mp3") && !heard.contains(".flac"), "{folder} {heard}");

    // One track id, two copies: `from` is all that says which is heard.
    let (status, bytes) = ann.bytes(&format!("/api/stream/cratarr/{}?from={from}", waiting[0])).await;
    assert_eq!(status, 200);
    assert!(is_mp3(&bytes), "the copy's MP3 as it lies");
    assert!(!is_mp3(&ann.bytes(&format!("/api/stream/cratarr/{}", waiting[0])).await.1), "with none named, the record's FLAC");

    // "Before adding" writes to the copy waiting alone, wherever the record plays from (Q20).
    let (kept, before) = (as_it_lies(&music.join(dir)), as_it_lies(&incoming.join(dir)));
    let sheet = ann.get(&format!("/api/file-changes/plans/intake:{id}?from={from}")).await;
    let record = sheet["records"].as_array().unwrap().first().unwrap_or_else(|| panic!("the rule has something to write: {sheet}"));
    let stamps = json!({ id: record["stamp"] });
    ann.ok(reqwest::Method::POST, &format!("/api/file-changes/plans/intake:{id}/apply?from={from}"), json!({ "albums": [id], "stamps": stamps })).await;
    s.until("the copy written", Duration::from_secs(60), || async { as_it_lies(&incoming.join(dir)) != before }).await;
    assert!(as_it_lies(&music.join(dir)) == kept, "nothing of the kept copy");
    assert_eq!(vorbis(&music.join(dir).join("01 Landing.flac"), "GENRE"), ["Electronic"]);
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// A request of bob's waits in Incoming, and Music holds the same record, so the record's id is on his list of what he may delete.
/// The right is his copy's: naming Music's is refused and leaves it there, and his own goes.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_requester_cannot_delete_the_kept_copy_end_to_end() {
    let root = temp("requester");
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_layout(&library);
    let (music, incoming) = (library.join("music"), library.join("incoming"));
    settings(&data, &[(&music, "main"), (&incoming, "incoming")], &library.join("own"));
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.ok(reqwest::Method::POST, "/api/users", json!({ "name": "ann", "password": "ann's password", "admin": true })).await;
    let ann = s.sign_in("ann", "ann's password").await;
    ann.ok(reqwest::Method::POST, "/api/users", json!({ "name": "bob", "password": "bob's password" })).await;
    let bob = s.sign_in("bob", "bob's password").await;
    let user = rusqlite::Connection::open(data.join("userdata.sqlite")).unwrap();
    user.busy_timeout(Duration::from_secs(10)).unwrap();
    user.execute("INSERT INTO request (user, album_key, artist_key, album, artist, at, status) VALUES ('bob', 'k', 'k', 'Both Shores', 'Salt Meridian', 1, 'approved')", []).unwrap();
    drop(user);
    let path = |p: &Path| p.to_string_lossy().to_string();
    let copies = |a: &Value| a["copies"].as_array().cloned().unwrap_or_default();
    let holds = |a: &Value, p: &Path| copies(a).iter().any(|c| c["library"].as_str() == Some(&*path(p)));
    s.until("both copies read", Duration::from_secs(120), || async { bob.albums().await.iter().any(|a| a["title"] == "Both Shores" && holds(a, &music) && holds(a, &incoming)) }).await;
    let album = bob.albums().await.into_iter().find(|a| a["title"] == "Both Shores").unwrap();
    let of = |p: &Path| copies(&album).into_iter().find(|c| c["library"].as_str() == Some(&*path(p))).unwrap();
    assert_eq!((of(&incoming)["deletable"].as_bool(), of(&music)["deletable"].as_bool()), (Some(true), Some(false)), "{album}");
    let view = bob.get("/api/albums").await;
    assert!(view["sources"][0]["deletable"].as_array().unwrap().contains(&album["id"]), "by id, the record is on his list: what must not count");

    let (dir, deleting) = ("Salt Meridian/Both Shores (2018)", format!("/api/albums/cratarr/{}/delete", album["id"].as_str().unwrap()));
    let kept = as_it_lies(&music.join(dir));
    assert_eq!(bob.send(reqwest::Method::POST, &deleting, json!({ "from": path(&music) })).await.0, 409, "not his");
    assert!(as_it_lies(&music.join(dir)) == kept, "the kept files are there");
    bob.ok(reqwest::Method::POST, &deleting, json!({ "from": path(&incoming) })).await;
    assert!(!incoming.join(dir).exists(), "his own request");
    assert!(as_it_lies(&music.join(dir)) == kept);
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// One folder over two discs in two libraries: nothing lies right in it, so what the folders' facts say cannot reach it, and a list
/// must not fall back on what the record plays from. Each lists its own format, and goes whole.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_copy_over_two_folders_lists_its_own_format_end_to_end() {
    let root = temp("two-folders");
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_layout(&library);
    let (music, incoming) = (library.join("music"), library.join("incoming"));
    let dir = "Salt Meridian/Both Shores (2018)";
    for folder in [music.join(dir), incoming.join(dir)] {
        let tracks = audio_under(&folder);
        for (n, track) in tracks.iter().enumerate() {
            let disc = folder.join(if n == 0 { "CD1" } else { "CD2" });
            std::fs::create_dir_all(&disc).unwrap();
            let file = Path::new(track).file_name().unwrap();
            std::fs::rename(folder.join(file), disc.join(file)).unwrap();
        }
    }
    settings(&data, &[(&music, "main"), (&incoming, "incoming")], &root.join("no-own-music"));
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    let path = |p: &Path| p.to_string_lossy().to_string();
    s.read_in("Both Shores", &music).await;
    let album = s.read_in("Both Shores", &incoming).await;
    let format = |library: &Path| album["copies"].as_array().unwrap().iter().find(|c| c["library"].as_str() == Some(&*path(library))).unwrap()["format"].clone();
    assert_eq!((format(&music), format(&incoming)), (json!("FLAC"), json!("MP3")), "{album}");
    let listed = s.incoming_album("Both Shores").await;
    assert_eq!(listed["format"], "MP3", "{listed}");

    let (kept, gone) = (as_it_lies(&music.join(dir)), s.ok(reqwest::Method::POST, &format!("/api/albums/cratarr/{}/delete", album["id"].as_str().unwrap()), json!({ "from": path(&incoming) })).await);
    assert_eq!(gone["title"], "Both Shores");
    assert!(!incoming.join(dir).exists(), "both discs, the folder they share");
    assert!(as_it_lies(&music.join(dir)) == kept);
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// CR17, the owner: the best copy in the kept libraries on plays, whoever's it is, and nothing is kept. Ann's own FLAC beats
/// Music's MP3 for her. Bob has her music off, so he hears Music's, though a FLAC waits in Incoming: nobody has checked it. His own
/// MP3 of another record loses to Music's FLAC. A stream that names a copy hears it for that one request, a library off still
/// winning.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn the_best_copy_plays_whoever_owns_it_and_nothing_is_kept_end_to_end() {
    let root = temp("best-copy");
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_layout(&library);
    let (music, second, incoming, own) = (library.join("music"), library.join("second"), library.join("incoming"), library.join("own"));
    let (kept, hers, waiting, aside) = (music.join(SHORES), own.join("ann").join(SHORES), incoming.join(SHORES), root.join("aside"));
    std::fs::remove_dir_all(&waiting).unwrap();
    std::fs::rename(second.join(SHORES), &waiting).unwrap();
    std::fs::rename(&kept, &aside).unwrap();
    std::fs::rename(&hers, &kept).unwrap();
    std::fs::rename(&aside, &hers).unwrap();
    settings(&data, &[(&music, "main"), (&second, "main"), (&incoming, "incoming")], &own);
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.ok(reqwest::Method::POST, "/api/users", json!({ "name": "ann", "password": "ann's password", "admin": true })).await;
    let ann = s.sign_in("ann", "ann's password").await;
    ann.ok(reqwest::Method::POST, "/api/users", json!({ "name": "bob", "password": "bob's password" })).await;
    let bob = s.sign_in("bob", "bob's password").await;
    let record = |list: Vec<Value>, title: &str| list.into_iter().find(|a| a["title"] == title);
    let holds = |a: Option<Value>, copies: usize| a.is_some_and(|a| a["copies"].as_array().is_some_and(|c| c.len() == copies));
    s.until("every copy read", Duration::from_secs(180), || async {
        let list = bob.albums().await;
        holds(record(list.clone(), "Both Shores"), 3) && holds(record(list, "No Papers"), 2)
    })
    .await;
    let plays = |a: &Value| a["copies"].as_array().unwrap().iter().find(|c| c["plays"] == true).map(|c| c["library"].as_str().unwrap().to_string());
    let first_song = |a: Value| a["rows"][0]["id"].as_str().unwrap().to_string();

    let (theirs, his) = (record(ann.albums().await, "Both Shores").unwrap(), record(bob.albums().await, "Both Shores").unwrap());
    assert_eq!(plays(&theirs), Some(id_of(&own.join("ann"))), "her FLAC, the best of the libraries she has on: {theirs}");
    assert_eq!(theirs["format"], "FLAC", "the card is what plays for her");
    assert_eq!(plays(&his), Some(id_of(&music)), "her music is off for him, and Incoming's FLAC is nobody's yet: {his}");
    assert_eq!(his["format"], "MP3");
    let song = first_song(ann.get(&format!("/api/albums/cratarr/{}", theirs["id"].as_str().unwrap())).await);
    let file = format!("/api/file/cratarr/{song}");
    let (flac, mp3) = (std::fs::read(hers.join("01 Landing.flac")).unwrap(), std::fs::read(kept.join("01 Landing.mp3")).unwrap());
    assert!(ann.bytes(&file).await.1 == flac, "her own FLAC, over Music's MP3");
    assert!(bob.bytes(&file).await.1 == mp3, "Music's MP3, on one track id");

    let papers = record(bob.albums().await, "No Papers").unwrap();
    assert_eq!(plays(&papers), Some(id_of(&music)), "a FLAC in Music over his own MP3: {papers}");
    let unsigned = first_song(bob.get(&format!("/api/albums/cratarr/{}", papers["id"].as_str().unwrap())).await);
    let (_, bytes) = bob.bytes(&format!("/api/file/cratarr/{unsigned}")).await;
    assert!(bytes == std::fs::read(music.join("Salt Meridian/No Papers (2018)/01 Unsigned.flac")).unwrap(), "what plays is Music's file");

    let stream = format!("/api/stream/cratarr/{song}");
    let named = |library: &Path| format!("{stream}?from={}", id_of(library).replace('/', "%2F"));
    assert!(is_mp3(&bob.bytes(&stream).await.1), "Music's");
    assert!(!is_mp3(&bob.bytes(&named(&incoming)).await.1), "the copy named, though it plays for no one");
    assert!(is_mp3(&bob.bytes(&stream).await.1), "for that one request: the next is the best again");
    assert!(is_mp3(&bob.bytes(&named(&own.join("ann"))).await.1), "a copy in a library off never plays, named or not");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// What `access::signed` adds to a path's address, read from where the server keeps its secret: a link with no account behind it,
/// as Kodi is given one.
fn signed(data: &Path, path: &str) -> String {
    use sha2::{Digest, Sha256};
    let db = rusqlite::Connection::open(data.join("sessions.sqlite")).unwrap();
    db.busy_timeout(Duration::from_secs(10)).unwrap();
    let secret: String = db.query_row("SELECT value FROM settings WHERE name = 'link_secret'", [], |r| r.get(0)).unwrap();
    let expires = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 3600;
    let mut key = [0u8; 64];
    key[..secret.len()].copy_from_slice(secret.as_bytes());
    let (mut inner, mut outer) = (Sha256::new(), Sha256::new());
    inner.update(key.map(|b| b ^ 0x36));
    inner.update(format!("{path}\n{expires}"));
    outer.update(key.map(|b| b ^ 0x5c));
    outer.update(inner.finalize());
    let sig: String = outer.finalize().iter().map(|b| format!("{b:02x}")).collect();
    format!("exp={expires}&sig={sig}")
}

/// Play from's server part: a track of such a queue names its copy on every address a player asks. `/api/file` is what the app
/// and Kodi play, and a Kodi's link is signed, with no account behind it. It counts for that one request.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_copy_named_is_heard_on_every_address_and_through_a_signed_link_end_to_end() {
    let root = temp("named-copy");
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_layout(&library);
    // A FLAC waiting, a second longer than Music's at its first track: its bytes and its length tell it.
    let (music, waiting) = (library.join("music"), library.join("second"));
    settings(&data, &[(&music, "main"), (&waiting, "incoming")], &root.join("no-own-music"));
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.ok(reqwest::Method::POST, "/api/users", json!({ "name": "ann", "password": "ann's password", "admin": true })).await;
    let ann = s.sign_in("ann", "ann's password").await;
    let both = |a: &Value| a["title"] == "Both Shores" && a["copies"].as_array().is_some_and(|c| c.len() == 2);
    s.until("both copies read", Duration::from_secs(120), || async { ann.albums().await.iter().any(both) }).await;
    let album = ann.albums().await.into_iter().find(|a| both(a)).unwrap();
    let song = ann.get(&format!("/api/albums/cratarr/{}", album["id"].as_str().unwrap())).await["rows"][0]["id"].as_str().unwrap().to_string();
    let (file, stream) = (format!("/api/file/cratarr/{song}"), format!("/api/stream/cratarr/{song}"));
    let from = format!("from={}", id_of(&waiting).replace('/', "%2F"));
    let (kept, named) = (std::fs::read(music.join(SHORES).join("01 Landing.flac")).unwrap(), std::fs::read(waiting.join(SHORES).join("01 Landing.flac")).unwrap());
    assert!(kept != named, "two files to tell apart");

    assert!(ann.bytes(&file).await.1 == kept, "the best copy");
    assert!(ann.bytes(&format!("{file}?{from}")).await.1 == named, "the copy named");
    assert!(ann.bytes(&file).await.1 == kept, "for that one request");
    let (best, asked) = (ann.bytes(&stream).await.1, ann.bytes(&format!("{stream}?{from}")).await.1);
    assert!(best != asked, "a stream of the other file");

    let wind = format!("{stream}/wind?from_ms=0&dir=forward&map=1");
    let (over_best, over_named) = (ann.get(&wind).await, ann.get(&format!("{wind}&{from}")).await);
    assert_ne!(over_best["length_ms"], over_named["length_ms"], "a wind through the other file");
    let rewind = format!("{stream}/rewind?from_ms=2500&ms=500");
    let (back_best, back_named) = (ann.bytes(&rewind).await, ann.bytes(&format!("{rewind}&{from}")).await);
    assert_eq!((back_best.0, back_named.0), (200, 200));
    assert!(back_best.1 != back_named.1, "a rewind of the other file");

    assert_eq!(s.anyone().bytes(&file).await.0, 401, "signing in is required");
    assert_eq!(s.anyone().bytes(&format!("{file}?exp=1&sig=0")).await.0, 401, "a link not signed, and the secret comes to be");
    let (status, bytes) = s.anyone().bytes(&format!("{file}?{from}&{}", signed(&data, &file))).await;
    assert!(status == 200 && bytes == named, "the copy named, with no one signed in: {status}");
    assert!(s.anyone().bytes(&format!("{file}?{}", signed(&data, &file))).await.1 == kept, "with none named, the row copy");
    let (status, bytes) = s.anyone().bytes(&format!("{stream}?{from}&{}", signed(&data, &stream))).await;
    assert!(status == 200 && bytes == asked, "a stream too: {status}");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// The owner, 2026-11-07: a folder of the very same release is the record from its first file. On real files through passes, its last
/// track first: one card, each file on the track it is, what plays as it was, the copy heard by `from` for what it holds and passed over
/// for what it does not, Replace refused as it would take tracks it lacks, a Delete taking its files alone, and its last file in, a copy
/// like any other.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_copy_arriving_file_by_file_is_the_record_from_its_first_file_end_to_end() {
    let root = temp("arriving");
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_layout(&library);
    let (music, incoming, aside) = (library.join("music"), library.join("incoming"), root.join("aside"));
    std::fs::rename(incoming.join(SHORES), &aside).unwrap();
    settings(&data, &[(&music, "main"), (&incoming, "incoming")], &root.join("no-own-music"));
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.ok(reqwest::Method::POST, "/api/users", json!({ "name": ANN.0, "password": ANN.1, "admin": true })).await;
    let ann = s.sign_in(ANN.0, ANN.1).await;
    let entries = || async { ann.albums().await.into_iter().filter(|a| a["title"] == "Both Shores").collect::<Vec<Value>>() };
    s.until("Music's copy read", Duration::from_secs(120), || async { !entries().await.is_empty() }).await;
    let id = entries().await[0]["id"].as_str().unwrap().to_string();
    let songs: Vec<String> = ann.get(&format!("/api/albums/cratarr/{id}")).await["rows"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap().to_string()).collect();
    assert_eq!(songs.len(), 3);
    let (kept, waiting) = (audio_under(&music.join(SHORES)), audio_under(&aside));
    let arrives = |file: &str| {
        std::fs::create_dir_all(incoming.join(SHORES)).unwrap();
        std::fs::copy(aside.join(file), incoming.join(SHORES).join(file)).unwrap();
    };
    let copies_held = |n: u64| {
        let (ann, entries, there) = (&ann, &entries, id_of(&incoming));
        async move {
            ask(ann, "startScan", "").await;
            built(ann).await;
            entries().await.first().is_some_and(|a| a["copies"].as_array().is_some_and(|c| c.iter().any(|c| c["library"] == there.as_str() && c["tracks"] == n)))
        }
    };

    arrives(&waiting[2]);
    s.until("its first file", Duration::from_secs(120), || copies_held(1)).await;
    let listed = entries().await;
    assert_eq!((listed.len(), listed[0]["id"].as_str()), (1, Some(id.as_str())), "the record you hold, one card: {listed:?}");
    assert_eq!(listed[0]["tracks"], 3, "listing what it lists");
    assert_eq!(tracks_at(&data, &incoming, SHORES), songs[2..], "the track it is");
    let plays: Vec<&Value> = listed[0]["copies"].as_array().unwrap().iter().filter(|c| c["plays"] == true).map(|c| &c["library"]).collect();
    assert_eq!(plays, [&json!(id_of(&music))], "what played plays on");

    // Play: Music's, and an Incoming copy only by `from`, which a track it does not hold passes over.
    let (kept_last, waiting_last) = (std::fs::read(music.join(SHORES).join(&kept[2])).unwrap(), std::fs::read(aside.join(&waiting[2])).unwrap());
    let from = format!("from={}", id_of(&incoming).replace('/', "%2F"));
    assert!(ann.bytes(&format!("/api/file/cratarr/{}", songs[2])).await.1 == kept_last, "Music's, as ever");
    assert!(ann.bytes(&format!("/api/file/cratarr/{}?{from}", songs[2])).await.1 == waiting_last, "the copy named, for the track it holds");
    let first = std::fs::read(music.join(SHORES).join(&kept[0])).unwrap();
    assert!(ann.bytes(&format!("/api/file/cratarr/{}?{from}", songs[0])).await.1 == first, "passed over for one it does not");

    // Replace would put three tracks' place in the hands of one: refused, and nothing moves.
    let (old, new) = (as_it_lies(&music.join(SHORES)), as_it_lies(&incoming.join(SHORES)));
    let replacing = format!("/api/incoming/replace/cratarr/{id}?{from}");
    let preview = ann.get(&replacing).await;
    assert!(!preview["refused"].is_null(), "two tracks of Music's have no match: {preview}");
    assert_eq!(ann.send(reqwest::Method::POST, &replacing, json!({ "from": id_of(&incoming) })).await.0, 409);
    assert!(as_it_lies(&music.join(SHORES)) == old && as_it_lies(&incoming.join(SHORES)) == new, "a refusal moves nothing");

    // A Delete takes its one file. The record goes on, and an Undo brings it back on the track it was.
    let gone = ann.ok(reqwest::Method::POST, &format!("/api/albums/cratarr/{id}/delete"), json!({ "from": id_of(&incoming) })).await;
    assert!(!incoming.join(SHORES).exists(), "the copy named went");
    assert!(as_it_lies(&music.join(SHORES)) == old, "and no other");
    s.until("the copy to go", Duration::from_secs(120), || async { entries().await.first().is_some_and(|a| a["copies"].as_array().is_some_and(|c| c.len() == 1)) }).await;
    assert_eq!(entries().await[0]["id"].as_str(), Some(id.as_str()));
    ann.ok(reqwest::Method::POST, "/api/undo", json!({ "tokens": [gone["undo"]] })).await;
    s.until("its Undo", Duration::from_secs(120), || copies_held(1)).await;
    assert_eq!(tracks_at(&data, &incoming, SHORES), songs[2..]);

    // The rest come in, in no order, and it is a copy like any other, on the record's tracks.
    arrives(&waiting[0]);
    s.until("its next file", Duration::from_secs(120), || copies_held(2)).await;
    arrives(&waiting[1]);
    s.until("its last file", Duration::from_secs(120), || copies_held(3)).await;
    assert_eq!(tracks_at(&data, &incoming, SHORES), songs, "each on the track it is");
    assert_eq!(entries().await.len(), 1);
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// The owner, 2026-11-07: "If it's owned by administrator, then only administrator can edit." A record in a main library is the
/// administrators', a copy of one's own beside it or not; a record only in one person's own music is theirs, and no one else's.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn who_may_set_a_records_facts_goes_by_whose_record_it_is_end_to_end() {
    let root = temp("whose");
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_layout(&library);
    settings(&data, &[(&library.join("music"), "main"), (&library.join("incoming"), "incoming")], &library.join("own"));
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.ok(reqwest::Method::POST, "/api/users", json!({ "name": "ann", "password": "ann's password", "admin": true })).await;
    let ann = s.sign_in("ann", "ann's password").await;
    ann.ok(reqwest::Method::POST, "/api/users", json!({ "name": "bob", "password": "bob's password" })).await;
    let bob = s.sign_in("bob", "bob's password").await;
    let all = ["Tide Tables", "No Papers", "Other Room", "Own Room", "Just Arrived"];
    s.until("every library read", Duration::from_secs(180), || async {
        let albums = ann.albums().await;
        all.iter().all(|t| albums.iter().any(|a| a["title"] == *t))
    })
    .await;
    let albums = ann.albums().await;
    let id = |title: &str| albums.iter().find(|a| a["title"] == title).unwrap()["id"].clone();
    let setting = |title: &str, year: u32| json!({ "albums": [id(title)], "set": { "year": year } });

    let (status, said) = bob.send(reqwest::Method::PUT, "/api/albums/overrides", setting("Tide Tables", 1950)).await;
    assert_eq!(status, 403, "Music's: {said}");
    assert_eq!(bob.send(reqwest::Method::PUT, "/api/albums/overrides", setting("No Papers", 1950)).await.0, 403, "his own copy gives him no say in Music's record");
    assert_eq!(bob.send(reqwest::Method::PUT, "/api/albums/overrides", setting("Own Room", 1950)).await.0, 403, "another's own music");
    assert_eq!(bob.send(reqwest::Method::PUT, "/api/albums/overrides", setting("Just Arrived", 1950)).await.0, 403, "Incoming's");
    let both = json!({ "albums": [id("Other Room"), id("Tide Tables")], "set": { "year": 1950 } });
    assert_eq!(bob.send(reqwest::Method::PUT, "/api/albums/overrides", both).await.0, 403, "all of a selection or none");
    let year_now = |albums: Vec<Value>, title: &str| albums.into_iter().find(|a| a["title"] == title).unwrap()["year"].clone();
    assert_ne!(year_now(ann.albums().await, "Other Room"), 1950, "none");

    bob.ok(reqwest::Method::PUT, "/api/albums/overrides", setting("Other Room", 1951)).await;
    assert_eq!(year_now(ann.albums().await, "Other Room"), 1951, "only in his own music: his to change");
    ann.ok(reqwest::Method::PUT, "/api/albums/overrides", setting("Tide Tables", 1952)).await;
    ann.ok(reqwest::Method::PUT, "/api/albums/overrides", setting("Other Room", 1953)).await;
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// The owner, 2026-11-07: an app sees every library, whatever the Library page's filter says. The filter stays on the account, and goes
/// on saying which copy plays.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn an_app_sees_every_library_whatever_the_librarys_filter_says_end_to_end() {
    let root = temp("apps");
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_layout(&library);
    let (music, second) = (library.join("music"), library.join("second"));
    settings(&data, &[(&music, "main"), (&second, "main")], &root.join("no-own-music"));
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.ok(reqwest::Method::POST, "/api/users", json!({ "name": ANN.0, "password": ANN.1, "admin": true })).await;
    let ann = s.sign_in(ANN.0, ANN.1).await;
    let shores = || async { ann.albums().await.into_iter().find(|a| a["title"] == "Both Shores" && a["copies"].as_array().is_some_and(|c| c.len() == 2)) };
    s.until("both copies read", Duration::from_secs(120), || async { shores().await.is_some() }).await;
    let folders = || async { ask(&ann, "getMusicFolders", "").await["musicFolders"]["musicFolder"].as_array().unwrap().iter().map(|f| f["id"].clone()).collect::<Vec<Value>>() };
    let (all, numbers) = (folders().await, ask(&ann, "getUser", &format!("username={}", ANN.0)).await["user"]["folder"].clone());
    assert_eq!((all.len(), numbers.as_array().map(Vec::len)), (2, Some(2)));

    let plays = |a: Value| a["copies"].as_array().unwrap().iter().find(|c| c["plays"] == true).unwrap()["library"].clone();
    let first = plays(shores().await.unwrap());
    let (now, other) = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64, if first == id_of(&music).as_str() { &second } else { &music });
    ann.ok(reqwest::Method::PUT, "/api/me/preferences/hidden_libraries", json!({ "value": [format!("cratarr/{}", first.as_str().unwrap())], "changed": now })).await;
    assert_eq!(folders().await, all, "its music folders stay");
    assert_eq!(ask(&ann, "getUser", &format!("username={}", ANN.0)).await["user"]["folder"], numbers, "and its account's");
    assert_eq!(plays(shores().await.unwrap()), id_of(other).as_str(), "the copy that plays still follows the switch");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// The environment starts a server and Settings runs it: a Lidarr removed stays removed and a password changed stays changed, across
/// restarts on the same environment.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn what_settings_say_stands_against_the_environment_end_to_end() {
    let root = temp("environment");
    let data = root.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let env = [("LIDARR_URL", "http://127.0.0.1:9"), ("LIDARR_API_KEY", "key"), ("CRATARR_SUBSONIC_USER", "ann"), ("CRATARR_SUBSONIC_PASSWORD", "from-the-environment")];
    let s = Server::start_with(&data, &env).await;
    let ann = s.sign_in("ann", "from-the-environment").await;
    assert_eq!(ann.get("/api/settings").await["lidarr"]["url"], "http://127.0.0.1:9", "the environment's, until Settings says");
    ann.ok(reqwest::Method::PUT, "/api/me/password", json!({ "current": "from-the-environment", "password": "chosen-here" })).await;
    ann.ok(reqwest::Method::PUT, "/api/settings", json!({ "sources": [], "lidarr": { "url": "", "api_key": "", "enabled": false } })).await;
    s.stop();

    let s = Server::start_with(&data, &env).await;
    let ann = s.sign_in("ann", "chosen-here").await;
    assert!(ann.get("/api/settings").await["lidarr"].is_null(), "removed stays removed");
    let said = s.anyone().get("/rest/ping?u=ann&p=from-the-environment&v=1.16.1&c=e2e").await;
    assert_eq!(said["subsonic-response"]["status"], "failed", "the environment's password is no longer the account's: {said}");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// A5: what no rule can pair, a title and a file name changed at once with no recording, leaves a track with plays and no file. It is on
/// Health's Missing list, and a person's word puts its plays and heart on the file it became.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_track_found_again_by_hand_keeps_what_it_held_end_to_end() {
    let root = temp("reconnect");
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_layout(&library);
    let music = library.join("music");
    settings(&data, &[(&music, "main")], &root.join("no-own-music"));
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.ok(reqwest::Method::POST, "/api/users", json!({ "name": ANN.0, "password": ANN.1, "admin": true })).await;
    let ann = s.sign_in(ANN.0, ANN.1).await;
    s.until("the library read", Duration::from_secs(120), || async { ann.albums().await.iter().any(|a| a["title"] == "Tide Tables") }).await;
    let old = tracks_at(&data, &music, TIDE)[0].clone();
    ann.ok(reqwest::Method::POST, &format!("/api/track/cratarr/{old}/favorite?on=true"), json!({})).await;
    ask(&ann, "scrobble", &format!("id={old}&time=1700000000000")).await;

    let (was, now) = (music.join(TIDE).join("01 Slack Water.flac"), music.join(TIDE).join("01 Low Water.flac"));
    set_vorbis(&was, "TITLE", "Low Water");
    std::fs::rename(&was, &now).unwrap();
    let catalogue = rusqlite::Connection::open(data.join("catalogue.sqlite")).unwrap();
    catalogue.busy_timeout(Duration::from_secs(10)).unwrap();
    let new = || catalogue.query_row("SELECT 'tr-' || track FROM files WHERE rel = ?1 AND track IS NOT NULL", [format!("{TIDE}/01 Low Water.flac")], |r| r.get::<_, String>(0)).ok();
    s.until("the file read as new", Duration::from_secs(120), || async {
        ask(&ann, "startScan", "").await;
        built(&ann).await;
        new().is_some()
    })
    .await;
    let chosen = new().unwrap();
    assert_ne!(chosen, old, "no rule pairs them");

    // Its day over: the gone row's end, as a day passing leaves it.
    catalogue.execute("UPDATE files SET gone_at = gone_at - 100000 WHERE gone_at IS NOT NULL", []).unwrap();
    let on_the_list = || async {
        let report = ann.get("/api/health/report").await;
        report["findings"].as_array().unwrap().iter().find(|f| f["key"] == "library:missing").map_or(Vec::new(), |f| f["items"].as_array().unwrap().iter().filter(|i| i["track"] == old.as_str()).cloned().collect::<Vec<Value>>())
    };
    s.until("it to retire onto the list", Duration::from_secs(120), || async {
        ask(&ann, "startScan", "").await;
        built(&ann).await;
        catalogue.query_row("SELECT retired_at IS NOT NULL FROM tracks WHERE 'tr-' || id = ?1", [&old], |r| r.get::<_, bool>(0)).unwrap() && !on_the_list().await.is_empty()
    })
    .await;
    let row = on_the_list().await.remove(0);
    assert_eq!((row["title"].as_str(), row["files"][0].as_str().map(|f| f.ends_with("01 Slack Water.flac"))), (Some("Slack Water"), Some(true)), "what it was and where it lay: {row}");
    assert!(row["since"].is_number(), "since when: {row}");

    ann.ok(reqwest::Method::POST, "/api/missing/reconnect", json!({ "track": old, "to": chosen })).await;
    assert!(on_the_list().await.is_empty(), "off the list");
    let held = ask(&ann, "getSong", &format!("id={chosen}")).await["song"].clone();
    assert_eq!(held["playCount"], 1, "its play: {held}");
    assert!(held["starred"].is_string(), "its heart: {held}");
    let again = ann.send(reqwest::Method::POST, "/api/missing/reconnect", json!({ "track": old, "to": chosen })).await;
    assert_eq!(again.0, 409, "a take stands: {:?}", again.1);
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// Replace: the new files take the place and the track ids of the old, which go to Music's trash. A copy missing a track is refused
/// and nothing moves. Its Undo gives each library its own files and each file its own id again.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn replace_takes_the_old_tracks_ids_and_its_undo_gives_both_back_end_to_end() {
    let (s, root, music, incoming) = on_music_and_incoming("replace").await;
    s.read_in("Both Shores", &music).await;
    s.read_in("Both Shores", &incoming).await;
    let replacing = |album: &Value| format!("/api/incoming/replace/cratarr/{}", album["id"].as_str().unwrap());

    let short = s.incoming_album("Short Measure").await;
    let dir = "Salt Meridian/Short Measure (2018)";
    let (old, new) = (as_it_lies(&music.join(dir)), as_it_lies(&incoming.join(dir)));
    let preview = s.get(&replacing(&short)).await;
    assert!(!preview["refused"].is_null(), "a track of Music's has no match: {preview}");
    assert_eq!(s.send(reqwest::Method::POST, &replacing(&short), json!({})).await.0, 409);
    assert!(as_it_lies(&music.join(dir)) == old && as_it_lies(&incoming.join(dir)) == new, "a refusal moves nothing");

    let album = s.incoming_album("Both Shores").await;
    let dir = "Salt Meridian/Both Shores (2018)";
    let (old_dir, new_dir) = (music.join(dir), incoming.join(dir));
    let (old, new) = (as_it_lies(&old_dir), as_it_lies(&new_dir));
    let songs = s.songs_of(&s.read_in("Both Shores", &music).await).await;
    let data = root.join("data");
    let arriving = tracks_at(&data, &incoming, dir);
    assert_eq!(arriving, songs, "the copy waiting holds the record's tracks");

    let preview = s.get(&replacing(&album)).await;
    assert_eq!((preview["tracks"].as_u64(), preview["added"].as_u64(), &preview["refused"]), (Some(3), Some(0), &Value::Null), "{preview}");
    let done = s.ok(reqwest::Method::POST, &replacing(&album), json!({})).await;
    assert_eq!(done["queued"], false, "{done}");
    let folder = music.join(done["folder"].as_str().unwrap());
    assert!(contents(&as_it_lies(&folder)) == contents(&new), "the new files not in Music");
    assert!(!new_dir.exists(), "still in Incoming");
    assert_eq!(in_the_trash(&music), 3, "the old files in Music's trash");

    // An MP3 here starts with its tag, a FLAC with its own mark: which of the two a track id is served from.
    s.until("Music to serve the new files", Duration::from_secs(120), || async { s.served(&songs[0]).await.starts_with(b"ID3") }).await;
    let shown = s.read_in("Both Shores", &music).await;
    assert_eq!(s.songs_of(&shown).await, songs, "the old tracks' ids taken over");

    let run = s.album_run("replace").await;
    let (status, undone) = s.undo_a_moment_later(run["id"].as_str().unwrap()).await;
    assert_eq!((status, &undone["undone"]), (200, &json!(true)), "{undone}");
    assert!(as_it_lies(&old_dir) == old, "Music's files not back as they were");
    assert!(as_it_lies(&new_dir) == new, "Incoming's files not back as they were");
    assert_eq!(in_the_trash(&music), 0);
    s.until("Music to serve the old files", Duration::from_secs(120), || async { s.served(&songs[0]).await.starts_with(b"fLaC") }).await;
    let shown = s.read_in("Both Shores", &music).await;
    assert_eq!(s.songs_of(&shown).await, songs);
    s.read_in("Both Shores", &incoming).await;
    assert_eq!(tracks_at(&data, &incoming, dir), arriving, "each file its own id again");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// A length's fix, on a FLAC that a whole decode heard end early: the file written whole, its header saying what was heard. Its Undo
/// gives back the file itself, byte for byte, which is what its copy beside the journal is for.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_length_fix_sets_what_was_heard_and_its_undo_gives_the_file_back_end_to_end() {
    let (s, root, music, data) = on_the_fixtures("length").await;
    let rel = "Glass Harbour/Tide Tables (2012)/01 Slack Water.flac";
    let track = music.join(rel);
    let before = std::fs::read(&track).unwrap();
    assert!(header_ms(&track).abs_diff(3_000) < 50, "{}", header_ms(&track));

    // What a decode leaves, put there by hand: the analysis pass waits two minutes, and for most of a library to have one.
    let db = rusqlite::Connection::open(data.join("catalogue.sqlite")).unwrap();
    db.busy_timeout(Duration::from_secs(10)).unwrap();
    let heard = "INSERT INTO analysis (folder, rel, size, mtime, version, duration_secs, analysed_at) SELECT folder, rel, size, mtime, 1, 1.0, 0 FROM files WHERE rel = ?1";
    assert_eq!(db.execute(heard, [rel]).unwrap(), 1);
    drop(db);

    s.write_sheet("fix:files:sound-breaks").await;
    let run = s.done_run(None).await;
    assert_eq!(run["failed"], json!([]), "{run}");
    assert!(header_ms(&track).abs_diff(1_000) < 50, "{}", header_ms(&track));
    assert!(std::fs::read(&track).unwrap() != before, "the file written");

    let (status, undone) = s.undo(run["id"].as_str().unwrap()).await;
    assert_eq!((status, &undone["undone"]), (200, &json!(true)), "{undone}");
    assert!(std::fs::read(&track).unwrap() == before, "its original not given back");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// A cover run to its end, as the kill above never lets it, each kind of album: a FLAC's picture beside its tracks goes and comes
/// back, an MP3's inside them goes and comes back, and where there was no picture beside, none is left there.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_cover_is_written_and_its_undo_puts_the_old_one_back_end_to_end() {
    let (s, root, music, _) = on_the_fixtures("cover-undo").await;
    let new = std::fs::read(music.join("Various Artists/Club Classics (2001)/cover.jpg")).unwrap();

    let dir = music.join("Glass Harbour/Tide Tables (2012)");
    let (cover, track) = (dir.join("cover.jpg"), dir.join("02 Neap.flac"));
    let old = std::fs::read(&cover).unwrap();
    assert_ne!(old, new);
    let style = vorbis(&track, "STYLE");
    let album = s.albums().await.into_iter().find(|a| a["title"] == "Tide Tables").unwrap();
    s.upload_cover(&album, &new).await;
    let run = s.done_run(None).await;
    assert_eq!((&run["what"], &run["failed"]), (&json!("cover"), &json!([])), "{run}");
    assert!(std::fs::read(&cover).unwrap() == new, "the new picture beside");
    assert!(pictures_inside(&track) == [new.clone()], "the new picture inside");
    let first = run["id"].as_str().unwrap();
    let (status, undone) = s.undo(first).await;
    assert_eq!((status, &undone["undone"]), (200, &json!(true)), "{undone}");
    assert!(std::fs::read(&cover).unwrap() == old, "the old picture beside");
    assert_eq!(vorbis(&track, "STYLE"), style, "its tags left alone");

    let dir = music.join("Various Artists/Late Train OST (2005)");
    let tracks: Vec<PathBuf> = audio_under(&dir).iter().map(|f| dir.join(f)).collect();
    let was = pictures_inside(&tracks[0]);
    assert!(was.len() == 1 && was[0] != new && !dir.join("cover.jpg").exists());
    let album = s.albums().await.into_iter().find(|a| a["title"] == "Late Train OST").unwrap();
    s.upload_cover(&album, &new).await;
    let run = s.done_run(Some(first)).await;
    assert_eq!((&run["what"], &run["failed"]), (&json!("cover"), &json!([])), "{run}");
    assert!(tracks.iter().all(|t| pictures_inside(t) == [new.clone()]), "the new picture alone inside every track");
    let (status, undone) = s.undo(run["id"].as_str().unwrap()).await;
    assert_eq!((status, &undone["undone"]), (200, &json!(true)), "{undone}");
    assert!(tracks.iter().all(|t| pictures_inside(t) == was), "the old picture alone inside every track");
    assert!(!dir.join("cover.jpg").exists(), "there was none beside");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// Finding 86: a list's cover comes at the size it asks, a track's row by its record's, made once and kept; the player and a panel
/// ask without a size, and get the picture whole, as it is in the files.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_list_gets_a_cover_at_its_size_and_the_player_the_whole_one_end_to_end() {
    let (s, root, music, _) = on_the_fixtures("sized-covers").await;
    let big = {
        let picture = image::RgbImage::from_fn(1200, 1200, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 90]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(picture).write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
        out.into_inner()
    };
    let album = s.albums().await.into_iter().find(|a| a["title"] == "Tide Tables").unwrap();
    let id = album["id"].as_str().unwrap().to_string();
    let stamp = || async { s.get("/api/covers/stamps").await[format!("cratarr/{id}")].clone() };
    let before = stamp().await;
    s.upload_cover(&album, &big).await;
    s.done_run(None).await;
    s.until("the new cover's stamp", Duration::from_secs(120), || async { stamp().await != before }).await;
    let whole = std::fs::read(music.join("Glass Harbour/Tide Tables (2012)/cover.jpg")).unwrap();
    let side = |bytes: &[u8]| {
        let shown = image::load_from_memory(bytes).unwrap();
        shown.width().max(shown.height())
    };
    assert_eq!(side(&whole), 1200);

    let (status, small) = s.anyone().bytes(&format!("/api/albums/cratarr/{id}/cover?size=320")).await;
    assert_eq!((status, side(&small)), (200, 320));
    let (status, got) = s.anyone().bytes(&format!("/api/albums/cratarr/{id}/cover")).await;
    assert_eq!(status, 200);
    assert!(got == whole, "the whole picture, as it is in the files");
    let song = s.songs_of(&album).await[0].clone();
    let (status, row) = s.anyone().bytes(&format!("/api/cover/cratarr/{song}?size=160")).await;
    assert_eq!((status, side(&row)), (200, 160));
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// lofty keeps a FLAC's pictures on the file, not in its comments: `retag::rewrite` moves them over, or none could be taken out.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_covers_undo_takes_its_picture_out_of_a_flac_end_to_end() {
    let (s, root, music, _) = on_the_fixtures("cover-flac-undo").await;
    let new = std::fs::read(music.join("Various Artists/Club Classics (2001)/cover.jpg")).unwrap();
    let track = music.join("Glass Harbour/Tide Tables (2012)/02 Neap.flac");
    assert!(pictures_inside(&track).is_empty());
    let album = s.albums().await.into_iter().find(|a| a["title"] == "Tide Tables").unwrap();
    s.upload_cover(&album, &new).await;
    let run = s.done_run(None).await;
    let (status, undone) = s.undo(run["id"].as_str().unwrap()).await;
    assert_eq!((status, &undone["undone"]), (200, &json!(true)), "{undone}");
    assert_eq!(pictures_inside(&track).len(), 0, "it had no picture inside");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// The same from the other side: a new cover on a FLAC that holds one takes its place, or the file keeps every picture it was ever given.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_new_cover_takes_the_place_of_the_old_one_inside_a_flac_end_to_end() {
    let (s, root, music, _) = on_the_fixtures("cover-flac-twice").await;
    let new = std::fs::read(music.join("Various Artists/Club Classics (2001)/cover.jpg")).unwrap();
    let dir = music.join("Noor Field/Two Rooms (2019)");
    let track = dir.join(&audio_under(&dir)[0]);
    assert_eq!(pictures_inside(&track).len(), 1);
    let album = s.albums().await.into_iter().find(|a| a["title"] == "Two Rooms").unwrap();
    s.upload_cover(&album, &new).await;
    s.done_run(None).await;
    assert_eq!(pictures_inside(&track).len(), 1, "one picture inside");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// Two copies of a record, on a layout: the server, where it all lies, the two libraries, its id and folder. The record's tracks
/// are both copies': each read as new, the second took the first's as the library was built.
async fn on_two_copies(name: &str, kinds: [&str; 2], second: &str) -> (Server, PathBuf, PathBuf, PathBuf, String) {
    let root = temp(name);
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_layout(&library);
    let (music, other) = (library.join("music"), library.join(second));
    settings(&data, &[(&music, kinds[0]), (&other, kinds[1])], &library.join("own"));
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.read_in("Both Shores", &music).await;
    let album = s.read_in("Both Shores", &other).await;
    let id = album["id"].as_str().unwrap().to_string();
    let songs = s.songs_of(&album).await;
    assert_eq!(songs.len(), 3, "each track once, however many copies");
    assert_eq!((tracks_at(&data, &music, SHORES), tracks_at(&data, &other, SHORES)), (songs.clone(), songs), "both copies hold the record's tracks");
    (s, root, music, other, id)
}

const SHORES: &str = "Salt Meridian/Both Shores (2018)";

/// An MP3 starts with its tag or a frame; a FLAC as `/api/stream` sends it does not.
fn is_mp3(bytes: &[u8]) -> bool {
    bytes.starts_with(b"ID3") || bytes.first() == Some(&0xff)
}

/// A library's path as `from` and `to` name it.
fn id_of(library: &Path) -> String {
    library.to_string_lossy().to_string()
}

/// How many albums the Library lists by a title: a record is one entry, however many copies it has.
async fn listed(s: &Server, title: &str) -> usize {
    s.albums().await.iter().filter(|a| a["title"] == title).count()
}

/// The files of a copy, each as it lies, in the order of its tracks.
fn tracks_as_they_lie(dir: &Path) -> Vec<Vec<u8>> {
    audio_under(dir).iter().map(|f| std::fs::read(dir.join(f)).unwrap()).collect()
}

/// Keep with the record already kept: the copy goes on being the record's, on its track ids, in a second kept library. The record
/// stays one entry, a track once. The Undo gives Incoming its copy back on the same ids, and no pair of ids is left behind.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn keep_makes_a_second_copy_on_the_records_tracks_and_its_undo_takes_it_back_end_to_end() {
    let root = temp("second-copy");
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_layout(&library);
    let (music, second, incoming) = (library.join("music"), library.join("second"), library.join("incoming"));
    std::fs::remove_dir_all(second.join(SHORES)).unwrap();
    settings(&data, &[(&music, "main"), (&second, "main"), (&incoming, "incoming")], &root.join("no-own-music"));
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.read_in("Both Shores", &music).await;
    let album = s.read_in("Both Shores", &incoming).await;
    let id = album["id"].as_str().unwrap().to_string();
    let songs = s.songs_of(&album).await;
    assert_eq!(tracks_at(&data, &incoming, SHORES), songs, "taken as it was read");
    let waiting = as_it_lies(&incoming.join(SHORES));

    let kept = s.ok(reqwest::Method::POST, &format!("/api/incoming/keep/cratarr/{id}"), json!({ "from": id_of(&incoming), "to": id_of(&second) })).await;
    let folder = kept["folder"].as_str().unwrap().to_string();
    assert!(!incoming.join(SHORES).exists(), "out of Incoming");
    let in_second = s.read_in("Both Shores", &second).await;
    assert_eq!(in_second["id"], album["id"], "the record's, not an album of its own");
    assert_eq!(tracks_at(&data, &second, &folder), songs, "its files keep the record's tracks");
    assert_eq!(s.songs_of(&in_second).await, songs, "each once");
    assert_eq!(listed(&s, "Both Shores").await, 1);
    s.until("Incoming to let it go", Duration::from_secs(120), || async { s.album_in("Both Shores", &incoming).await.is_none() }).await;

    let run = s.album_run("keep").await;
    let (status, undone) = s.undo_a_moment_later(run["id"].as_str().unwrap()).await;
    assert_eq!((status, &undone["undone"]), (200, &json!(true)), "{undone}");
    assert!(as_it_lies(&incoming.join(SHORES)) == waiting, "not back as it was");
    assert!(!second.join(&folder).exists(), "still in the second library");
    let back = s.read_in("Both Shores", &incoming).await;
    assert_eq!((&back["id"], s.songs_of(&back).await), (&album["id"], songs.clone()));
    assert_eq!(tracks_at(&data, &incoming, SHORES), songs, "no pair left behind");
    s.until("the second library to let it go", Duration::from_secs(120), || async { s.album_in("Both Shores", &second).await.is_none() }).await;
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// Delete of each copy in turn, on one set of track ids: what plays goes on, or falls to the copy left, and an id is never lost.
/// A file served is the file of the copy that plays, byte for byte.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_delete_of_each_copy_in_turn_leaves_the_track_playing_from_the_other_end_to_end() {
    let (s, root, music, incoming, id) = on_two_copies("each-copy", ["main", "incoming"], "incoming").await;
    let album = json!({ "id": id });
    let songs = s.songs_of(&album).await;
    let (kept, waiting) = (tracks_as_they_lie(&music.join(SHORES)), tracks_as_they_lie(&incoming.join(SHORES)));
    assert_ne!(kept[0], waiting[0], "two copies of one track, told apart by their bytes");
    let deleting = format!("/api/albums/cratarr/{id}/delete");
    let served = |n: usize| {
        let (s, song) = (&s, songs[n].clone());
        async move { s.anyone().bytes(&format!("/api/file/cratarr/{song}")).await }
    };
    assert!(served(0).await.1 == kept[0], "Music's plays");

    // The copy that does not play: nothing changes for what plays, and the record keeps its tracks.
    let gone = s.ok(reqwest::Method::POST, &deleting, json!({ "from": id_of(&incoming) })).await;
    assert!(!incoming.join(SHORES).exists(), "the copy named went");
    s.until("Incoming to let its copy go", Duration::from_secs(120), || async { s.album_in("Both Shores", &incoming).await.is_none() }).await;
    assert!(served(0).await.1 == kept[0], "plays on");
    assert_eq!(s.songs_of(&album).await, songs, "the record's tracks stand");
    s.ok(reqwest::Method::POST, "/api/undo", json!({ "tokens": [gone["undo"]] })).await;
    s.read_in("Both Shores", &incoming).await;
    assert_eq!(tracks_at(&root.join("data"), &incoming, SHORES), songs, "back on the record's tracks");

    // The copy that plays: every track falls to the copy left, under its id.
    let gone = s.ok(reqwest::Method::POST, &deleting, json!({ "from": id_of(&music) })).await;
    assert!(!music.join(SHORES).exists(), "the copy named went");
    s.until("Music to let its copy go", Duration::from_secs(120), || async { s.album_in("Both Shores", &music).await.is_none() }).await;
    s.until("the track to play from the copy left", Duration::from_secs(120), || async { served(0).await.1 == waiting[0] }).await;
    assert_eq!(s.songs_of(&album).await, songs, "an id is the track's, whichever copy holds it");
    assert_eq!(listed(&s, "Both Shores").await, 1);
    s.ok(reqwest::Method::POST, "/api/undo", json!({ "tokens": [gone["undo"]] })).await;
    s.read_in("Both Shores", &music).await;
    s.until("Music's copy to play again", Duration::from_secs(120), || async { served(1).await.1 == kept[1] }).await;
    assert_eq!(tracks_at(&root.join("data"), &music, SHORES), songs);
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// A Move of a copy on shared tracks: the record's track goes on, and `from` hears the copy where it went.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_moved_copy_stays_on_the_records_tracks_and_plays_from_where_it_went_end_to_end() {
    let root = temp("moved-copy");
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_layout(&library);
    let (music, second, incoming) = (library.join("music"), library.join("second"), library.join("incoming"));
    std::fs::remove_dir_all(second.join(SHORES)).unwrap();
    settings(&data, &[(&music, "main"), (&second, "main"), (&incoming, "incoming")], &root.join("no-own-music"));
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.read_in("Both Shores", &music).await;
    let album = s.read_in("Both Shores", &incoming).await;
    let (id, songs) = (album["id"].as_str().unwrap().to_string(), s.songs_of(&album).await);
    let moved = s.ok(reqwest::Method::POST, &format!("/api/albums/cratarr/{id}/move"), json!({ "from": id_of(&incoming), "to": id_of(&second) })).await;
    let folder = moved["folder"].as_str().unwrap().to_string();
    s.read_in("Both Shores", &second).await;
    assert_eq!(tracks_at(&data, &second, &folder), songs, "the same tracks where it went");
    let from = id_of(&second).replace('/', "%2F");
    let (status, bytes) = s.anyone().bytes(&format!("/api/stream/cratarr/{}?from={from}", songs[0])).await;
    assert_eq!(status, 200);
    assert!(is_mp3(&bytes), "the moved copy's MP3, where the record plays a FLAC");
    assert!(!is_mp3(&s.anyone().bytes(&format!("/api/stream/cratarr/{}", songs[0])).await.1));
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// A track's own word goes into its file in every copy, by the track and not the file's name, and into no other track's. One
/// Undo takes it out of them all.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_tracks_tag_reaches_its_file_in_every_copy_by_its_track_end_to_end() {
    let root = temp("tag-by-track");
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    make_layout(&library);
    let (music, incoming) = (library.join("music"), library.join("incoming"));
    // Named as one copy's owner likes: only the track says which file this is.
    let (renamed, other) = (incoming.join(SHORES).join("02 By Another Name.mp3"), incoming.join(SHORES).join("03 Return.mp3"));
    std::fs::rename(incoming.join(SHORES).join("02 Crossing.mp3"), &renamed).unwrap();
    settings(&data, &[(&music, "main"), (&incoming, "incoming")], &root.join("no-own-music"));
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.read_in("Both Shores", &music).await;
    let album = s.read_in("Both Shores", &incoming).await;
    let songs = s.songs_of(&album).await;
    assert_eq!(tracks_at(&data, &incoming, SHORES), songs, "one set of tracks, whatever the files are called");
    let genre = |track: &Path| {
        use lofty::prelude::*;
        let file = lofty::read_from_path(track).unwrap();
        file.tags().iter().find_map(|t| t.get_string(lofty::tag::ItemKey::Genre).map(str::to_string))
    };
    let (crossing, return_) = (music.join(SHORES).join("02 Crossing.flac"), music.join(SHORES).join("03 Return.flac"));
    assert_eq!(genre(&renamed).as_deref(), Some("Electronic"));

    s.ok(reqwest::Method::PUT, &format!("/api/track/cratarr/{}/words", songs[1]), json!({ "genre": ["Dub"] })).await;
    let sheet = "fix:words:genre";
    s.until("the word on the sheet", Duration::from_secs(60), || async { s.get(&format!("/api/file-changes/plans/{sheet}")).await["records"].as_array().is_some_and(|r| !r.is_empty()) }).await;
    s.write_sheet(sheet).await;
    let run = s.done_run(None).await;
    assert_eq!(run["failed"], json!([]), "{run}");
    assert_eq!((genre(&crossing).as_deref(), genre(&renamed).as_deref()), (Some("Dub"), Some("Dub")), "its file in both copies");
    assert_eq!((genre(&return_).as_deref(), genre(&other).as_deref()), (Some("Electronic"), Some("Electronic")), "and no other track's");
    let (status, undone) = s.undo(run["id"].as_str().unwrap()).await;
    assert_eq!((status, &undone["undone"]), (200, &json!(true)), "{undone}");
    assert_eq!((genre(&crossing).as_deref(), genre(&renamed).as_deref()), (Some("Electronic"), Some("Electronic")), "one Undo for both");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// Q8, the owner: a track added to one of a record's copies splits them, and the copy with more tracks keeps the record, its id and
/// what points at it. On real files, through a pass: the copy left behind gets tracks of its own and lists every file, and a play on
/// the record's track stays there.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_track_added_to_one_copy_splits_the_record_end_to_end() {
    let (s, root, music, second, id) = on_two_copies("added", ["main", "main"], "second").await;
    let data = root.join("data");
    let songs = s.songs_of(&json!({ "id": id })).await;
    let user = rusqlite::Connection::open(data.join("userdata.sqlite")).unwrap();
    user.busy_timeout(Duration::from_secs(10)).unwrap();
    user.execute("INSERT INTO annotation (user, item, kind, play_count, own_count) VALUES ('me', ?1, 'song', 3, 3)", [&songs[0]]).unwrap();

    let added = second.join(SHORES).join("04 Encore.flac");
    std::fs::copy(second.join(SHORES).join("03 Return.flac"), &added).unwrap();
    set_vorbis(&added, "TITLE", "Encore");
    set_vorbis(&added, "TRACKNUMBER", "4");
    // A Delete of something else asks for a pass, which a new file outside Cratarr waits for.
    let far = s.read_in("Far Bank", &second).await;
    s.ok(reqwest::Method::POST, &format!("/api/albums/cratarr/{}/delete", far["id"].as_str().unwrap()), json!({})).await;
    s.until("the track to count", Duration::from_secs(120), || async { s.get(&format!("/api/albums/cratarr/{id}")).await["rows"].as_array().is_some_and(|r| r.len() == 4) }).await;
    s.until("the other copy to leave", Duration::from_secs(120), || async { s.album_in("Both Shores", &music).await.is_some_and(|a| a["id"] != json!(id)) }).await;
    let left = s.album_in("Both Shores", &music).await.unwrap();
    let kept = s.album_in("Both Shores", &second).await.unwrap();
    assert_eq!(kept["id"], json!(id), "the copy with more tracks keeps the record");
    assert_eq!(s.songs_of(&kept).await[..3], songs[..], "and its tracks");
    let own = s.songs_of(&left).await;
    assert_eq!(own.len(), 3, "every file of the other is listed");
    assert!(own.iter().all(|t| !songs.contains(t)), "on tracks of its own: {own:?} {songs:?}");
    assert_eq!(tracks_at(&data, &music, SHORES), own);
    let plays: i64 = user.query_row("SELECT play_count FROM annotation WHERE item = ?1", [&songs[0]], |r| r.get(0)).unwrap();
    assert_eq!(plays, 3, "what points at the record stays with it");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// A track's release type, whatever its format calls the field.
fn release_type(track: &Path) -> Option<String> {
    use lofty::prelude::*;
    let file = lofty::read_from_path(track).unwrap();
    file.tags().iter().find_map(|t| t.get_string(lofty::tag::ItemKey::MusicBrainzReleaseType).map(str::to_string))
}

/// A cover is the record's: the picture beside and inside every copy, each copy's own back with its Undo, FLAC and MP3.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_cover_reaches_every_copy_and_its_undo_gives_each_its_own_end_to_end() {
    let (s, root, music, incoming, id) = on_two_copies("cover-copies", ["main", "incoming"], "incoming").await;
    let new = std::fs::read(music.join("Various Artists/Club Classics (2001)/cover.jpg")).unwrap();
    let (kept, waiting) = (music.join(SHORES), incoming.join(SHORES));
    std::fs::write(kept.join("cover.jpg"), b"\xff\xd8the kept copy's").unwrap();
    let files: Vec<PathBuf> = [&kept, &waiting].iter().flat_map(|d| audio_under(d).into_iter().map(|f| d.join(f)).collect::<Vec<_>>()).collect();
    assert_eq!(files.len(), 6);
    let was: Vec<Vec<Vec<u8>>> = files.iter().map(|t| pictures_inside(t)).collect();

    s.upload_cover(&json!({ "id": id }), &new).await;
    let run = s.done_run(None).await;
    assert_eq!((&run["what"], &run["failed"]), (&json!("cover"), &json!([])), "{run}");
    assert!(std::fs::read(kept.join("cover.jpg")).unwrap() == new && std::fs::read(waiting.join("cover.jpg")).unwrap() == new, "the new picture beside both");
    assert!(files.iter().all(|t| pictures_inside(t) == [new.clone()]), "the new picture alone inside every track of both");

    let (status, undone) = s.undo(run["id"].as_str().unwrap()).await;
    assert_eq!((status, &undone["undone"]), (200, &json!(true)), "{undone}");
    assert!(std::fs::read(kept.join("cover.jpg")).unwrap().ends_with(b"the kept copy's"), "its own back");
    assert!(!waiting.join("cover.jpg").exists(), "there was none beside");
    assert!(files.iter().map(|t| pictures_inside(t)).collect::<Vec<_>>() == was, "each track as it was");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// A fact of the record goes into every copy's files, and one Undo takes it out of all of them.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_records_fact_reaches_every_copy_and_its_undo_takes_it_out_end_to_end() {
    let (s, root, music, incoming, _) = on_two_copies("facts-copies", ["main", "incoming"], "incoming").await;
    let files: Vec<PathBuf> = [music.join(SHORES), incoming.join(SHORES)].iter().flat_map(|d| audio_under(d).into_iter().map(|f| d.join(f)).collect::<Vec<_>>()).collect();
    assert!(files.iter().all(|t| release_type(t).is_none()));
    s.ok(reqwest::Method::PUT, "/api/settings/file-facts", json!({ "on": true })).await;
    s.write_the_sheet().await;
    let run = s.done_run(None).await;
    assert_eq!(run["failed"], json!([]), "{run}");
    let written: Vec<Option<String>> = files.iter().map(|t| release_type(t)).collect();
    assert!(written[0].is_some() && written.iter().all(|t| *t == written[0]), "the one type in every file of both: {written:?}");
    let (status, undone) = s.undo(run["id"].as_str().unwrap()).await;
    assert_eq!((status, &undone["undone"]), (200, &json!(true)), "{undone}");
    assert!(files.iter().all(|t| release_type(t).is_none()), "taken out of them all");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// A length is one file's: the copy that does not play is fixed, and the copy that plays is not touched.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_length_fix_stays_in_its_file_end_to_end() {
    let (s, root, music, second, _) = on_two_copies("length-copies", ["main", "main"], "second").await;
    // Three seconds in both copies: a track of two is no shorter than it says at a second heard, within Health's slack.
    let rel = format!("{SHORES}/02 Crossing.flac");
    // By quality either can play, and on the copy that plays a fix by the track would find the same file.
    let plays = s.read_in("Both Shores", &music).await["library"].as_str().unwrap().to_string();
    let (fixed, playing) = if plays == music.to_string_lossy() { (&second, &music) } else { (&music, &second) };
    let (track, other) = (fixed.join(&rel), playing.join(&rel));
    let (before, kept) = (std::fs::read(&track).unwrap(), std::fs::read(&other).unwrap());

    // What a decode leaves, put there by hand, as in the length fix above: on that file alone.
    let db = rusqlite::Connection::open(root.join("data/catalogue.sqlite")).unwrap();
    db.busy_timeout(Duration::from_secs(10)).unwrap();
    let heard = "INSERT INTO analysis (folder, rel, size, mtime, version, duration_secs, analysed_at)
                 SELECT f.folder, f.rel, f.size, f.mtime, 1, 1.0, 0 FROM files f JOIN folders d ON d.id = f.folder WHERE f.rel = ?1 AND d.path = ?2";
    assert_eq!(db.execute(heard, [rel.as_str(), &fixed.to_string_lossy()]).unwrap(), 1);
    drop(db);

    s.write_sheet("fix:files:sound-breaks").await;
    let run = s.done_run(None).await;
    assert_eq!(run["failed"], json!([]), "{run}");
    assert!(header_ms(&track).abs_diff(1_000) < 50, "{}", header_ms(&track));
    assert!(std::fs::read(&other).unwrap() == kept, "the other copy left alone");
    let (status, undone) = s.undo(run["id"].as_str().unwrap()).await;
    assert_eq!((status, &undone["undone"]), (200, &json!(true)), "{undone}");
    assert!(std::fs::read(&track).unwrap() == before, "its original given back");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// The one lock, seen: one reading of the list shows a run writing and a second, of another kind, waiting, not done. The second has
/// touched nothing yet, and takes its turn once the first is through.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_second_run_waits_while_the_first_writes_end_to_end() {
    let (s, root, music, _) = on_the_fixtures("waiting").await;
    s.ok(reqwest::Method::PUT, "/api/settings/file-facts", json!({ "on": true })).await;
    let cover = music.join("Glass Harbour/Tide Tables (2012)/cover.jpg");
    let (old, new) = (std::fs::read(&cover).unwrap(), std::fs::read(music.join("Various Artists/Club Classics (2001)/cover.jpg")).unwrap());
    let album = s.albums().await.into_iter().find(|a| a["title"] == "Tide Tables").unwrap();

    s.write_the_sheet().await;
    // Albums go at least 50 ms apart, so asked this often the run is caught with most of them to go.
    let asked = Instant::now();
    while s.get("/api/file-changes/runs").await[0]["state"] != "writing" {
        assert!(asked.elapsed() < Duration::from_secs(60), "waited for the run to get going");
    }
    let second = s.upload_cover(&album, &new).await;
    let runs = s.get("/api/file-changes/runs").await;
    let state = |what: &str| runs.as_array().unwrap().iter().find(|r| r["what"] == what).map(|r| r["state"].clone());
    assert_eq!((state("types"), state("cover")), (Some(json!("writing")), Some(json!("waiting"))), "{runs}");
    assert_eq!(second["state"], "waiting", "{second}");
    assert!(std::fs::read(&cover).unwrap() == old, "nothing written while it waits");

    s.until("the second run's turn", Duration::from_secs(120), || async {
        let runs = s.get("/api/file-changes/runs").await;
        runs.as_array().unwrap().iter().any(|r| r["what"] == "cover" && r["state"] == "done" && r["id"].as_str().unwrap().starts_with('r'))
    })
    .await;
    assert!(std::fs::read(&cover).unwrap() == new, "written in its turn");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}

/// The account of the proof below, and the albums it is given: each case of what a listener can lose, by where it lies.
const ANN: (&str, &str) = ("ann", "ann's password");
/// A second account, with libraries switched off.
const BO: (&str, &str) = ("bo", "bo's password");
const TIDE: &str = "Glass Harbour/Tide Tables (2012)";
const PARTIAL: &str = "Glass Harbour/Partial Shore (2011)";
const CLUB: &str = "Various Artists/Club Classics (2001)";
const JUNK: &str = "Orbit Lane/Junk Dates (1)";
const CUT: &str = "Salt Meridian/Long Cut (2018)";
const BONUS: &str = "Salt Meridian/Bonus Round (2018)";
const ARRIVED: &str = "Salt Meridian/Just Arrived (2018)";
const MEASURE: &str = "Salt Meridian/Short Measure (2018)";

/// How long what a start or a change sets going may take, on a PC running the whole suite at once.
const SETTLES: Duration = Duration::from_secs(60);

/// One library of `Shelves`: where it lies now, its kind, and what the proof calls it, whatever its folder is called.
struct Shelf {
    at: PathBuf,
    kind: &'static str,
    name: &'static str,
}

/// A server's libraries in Settings' order, under the build's own folder: Settings takes only a folder inside a mount
/// (`chosen_folders_ok`), and the temporary folder is none (`saved`).
struct Shelves {
    root: PathBuf,
    data: PathBuf,
    list: Vec<Shelf>,
}

impl Shelves {
    /// The whole layout's two main libraries and its Incoming, nothing read yet.
    fn made(name: &str) -> Shelves {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("cratarr-e2e-{name}"));
        std::fs::remove_dir_all(&root).ok();
        let (library, data) = (root.join("library"), root.join("data"));
        std::fs::create_dir_all(&data).unwrap();
        make_layout(&library);
        // A cover in the library that goes first, and a recording to know a file by, as a tagger leaves one.
        std::fs::copy(library.join("music").join(TIDE).join("cover.jpg"), library.join("second").join(FAR).join("cover.jpg")).unwrap();
        set_vorbis(&library.join("music").join(JUNK).join("01 Zero.flac"), "MUSICBRAINZ_TRACKID", "00000000-0000-4000-8000-000000000301");
        let shelf = |folder: &str, kind, name| Shelf { at: library.join(folder), kind, name };
        let list = vec![shelf("music", "main", "music"), shelf("second", "main", "second"), shelf("incoming", "incoming", "incoming")];
        let shelves = Shelves { root, data, list };
        shelves.write();
        shelves
    }

    fn at(&self, name: &str) -> &Path {
        &self.list.iter().find(|s| s.name == name).unwrap().at
    }

    fn libraries(&self) -> Vec<(&Path, &str)> {
        self.list.iter().map(|s| (s.at.as_path(), s.kind)).collect()
    }

    /// The settings file as they stand: a person's edit while the server is down.
    fn write(&self) {
        settings(&self.data, &self.libraries(), &self.root.join("no-own-music"));
    }

    /// Settings as they stand, sent as the page sends them, and the server that goes on. Settings takes only a folder inside a mount, and
    /// whether the build's folder is one goes by how the build runs: a stage of the Dockerfile may read its cache as a layer, no mount. There
    /// the settings file and a start stand in, as for the suite's other tests, and only what a save does on a running server goes unasked.
    async fn saved(&self, s: Server) -> Server {
        let sent = json!({ "sources": [own_source(&self.libraries(), &self.root.join("no-own-music"))] });
        let (status, said) = s.sign_in(ANN.0, ANN.1).await.send(reqwest::Method::PUT, "/api/settings", sent).await;
        if (200..300).contains(&status) {
            return s;
        }
        assert!(said.to_string().contains("cannot see"), "{status} {said}");
        println!("Settings cannot see {}: the settings file and a start stand in", self.root.display());
        self.write();
        restarted(s, &self.data).await
    }

    /// A library's folder under another name, as a mount renamed. What Settings says is the caller's.
    fn rename(&mut self, name: &str, to: &str) {
        let shelf = self.list.iter_mut().find(|s| s.name == name).unwrap();
        let to = shelf.at.with_file_name(to);
        std::fs::rename(&shelf.at, &to).unwrap();
        shelf.at = to;
    }

    /// An answer with each library by what the proof calls it, so a name as it stands now reads the same as before, and an answer
    /// naming a place a library left reads as what it is.
    fn plain(&self, answer: &Value) -> Value {
        let mut shelves: Vec<&Shelf> = self.list.iter().collect();
        // Longest first: a library's path can start another's.
        shelves.sort_by_key(|s| std::cmp::Reverse(s.at.as_os_str().len()));
        let text = shelves.iter().fold(answer.to_string(), |text, s| text.replace(&*s.at.to_string_lossy(), &format!("<{}>", s.name)));
        serde_json::from_str(&text).unwrap()
    }
}

/// What ann did with the library, by what `held_by` looks it up.
struct Mine {
    playlists: Vec<String>,
    crate_id: String,
    heard: Vec<String>,
    shaped: Vec<String>,
    covered: Vec<String>,
}

/// The body of a Subsonic answer to ann's app. An answer says a failure in itself, with a 200.
async fn ask(who: &Caller<'_>, method: &str, query: &str) -> Value {
    let sign_in = format!("u={}&p={}&v=1.16.1&c=e2e", ANN.0, ANN.1.replace(' ', "%20").replace('\'', "%27"));
    who.server.anyone().get(&format!("/rest/{method}?{sign_in}&{query}")).await["subsonic-response"].clone()
}

/// `answer`'s `fields`, those it has.
fn only(answer: &Value, fields: &[&str]) -> Value {
    Value::Object(fields.iter().filter_map(|f| Some((f.to_string(), answer.get(*f)?.clone()))).collect())
}

/// A list's ids, in order of the ids: `list` is an array of objects holding one.
fn ids_in(list: &Value) -> Vec<String> {
    in_order(list.as_array().into_iter().flatten().map(|x| x["id"].as_str().unwrap().to_string()).collect())
}

/// The ids of a list's rows that have one, in order of the ids: while a library is away a row can name what no track holds.
fn shown_ids(list: &Value) -> Vec<String> {
    in_order(list["rows"].as_array().into_iter().flatten().filter_map(|r| r["id"].as_str().map(str::to_string)).collect())
}

fn print_of(bytes: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::hash::DefaultHasher::new();
    bytes.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// What a file sounds like, whatever its tags say: its bytes past them, a FLAC's metadata blocks and an MP3's ID3v2. What tells that a
/// play is still on the same music after a retag.
fn sound_of(bytes: &[u8]) -> String {
    let mut at = 0;
    if bytes.starts_with(b"fLaC") {
        at = 4;
        loop {
            let (last, length) = (bytes[at] & 0x80 != 0, u32::from_be_bytes([0, bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize);
            at += 4 + length;
            if last {
                break;
            }
        }
    } else if bytes.starts_with(b"ID3") {
        at = 10 + bytes[6..10].iter().fold(0, |n, b| n << 7 | usize::from(*b));
    }
    print_of(&bytes[at..])
}

/// The album of `title` with a copy in `library`.
async fn album_id(who: &Caller<'_>, title: &str, library: &Path) -> String {
    let at = library.to_string_lossy();
    let there = |a: &Value| a["copies"].as_array().is_some_and(|c| c.iter().any(|c| c["library"].as_str() == Some(&*at)));
    let found = who.albums().await.into_iter().find(|a| a["title"] == title && there(a));
    found.unwrap_or_else(|| panic!("{title} in {at}"))["id"].as_str().unwrap().to_string()
}

/// The library built from what the catalogue has now, once the pass after a start is over: left alone, a start's first look at the
/// catalogue is a minute and a half away (`FIRST_AFTER`). A fact reset that was never set changes nothing, and looks at once.
async fn built(who: &Caller<'_>) {
    let start = Instant::now();
    loop {
        let progress = who.get("/api/catalogue").await["progress"].clone();
        if progress["doing"].is_null() && !progress["last_pass"].is_null() {
            break;
        }
        if start.elapsed() > SETTLES * 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if let Some(album) = who.albums().await.first() {
        who.send(reqwest::Method::PUT, "/api/albums/overrides", json!({ "albums": [album["id"]], "set": { "year": null } })).await;
    }
}

/// A file's analysis as the first pass over a library brings it in, its waveform all `level`.
async fn analysed(who: &Caller<'_>, library: &Path, rel: &str, level: u8) {
    let meta = std::fs::metadata(library.join(rel)).unwrap();
    let mtime = meta.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let analysis = json!({ "version": 1, "duration_secs": 2.0, "loudness_lufs": -12.0, "track_gain_db": -6.0, "track_peak": 0.98, "blocks": [[520, 30]], "bpm": 128.0, "bpm_confidence": 0.9, "waveform": vec![level; 512] });
    let line = json!({ "rel": rel, "size": meta.len(), "mtime": mtime, "analysis": analysis }).to_string();
    let sent = who.request(reqwest::Method::POST, &format!("/api/analysis/import?folder={}", library.display())).body(line).send().await.unwrap();
    let counts: Value = sent.json().await.unwrap();
    assert_eq!(counts["stored"], 1, "{rel}: {counts}");
}

/// An account's listening over the layout, each thing on what it is likeliest to lose it by: a track of a record in three libraries, one
/// in one library alone, and a track of a record tagged just like one in another library, which only its id tells apart.
async fn listen(who: &Caller<'_>, shelves: &Shelves) -> Mine {
    let (music, second, incoming, data) = (shelves.at("music"), shelves.at("second"), shelves.at("incoming"), &shelves.data);
    let (tide, shores, partial) = (tracks_at(data, music, TIDE), tracks_at(data, music, SHORES), tracks_at(data, music, PARTIAL));
    let (club, junk) = (tracks_at(data, music, CLUB), tracks_at(data, music, JUNK));
    let (far, cut, bonus) = (tracks_at(data, second, FAR), tracks_at(data, second, CUT), tracks_at(data, second, BONUS));
    let (arrived, measure) = (tracks_at(data, incoming, ARRIVED), tracks_at(data, incoming, MEASURE));

    // Ten tracks: Insights' list of them holds ten, and past that a title changed moves where it is cut.
    let heard = [&tide[0], &tide[0], &far[0], &cut[0], &bonus[3], &shores[0], &arrived[0], &measure[0], &partial[0], &junk[0], &club[0]];
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    for (n, song) in heard.iter().enumerate() {
        ask(who, "scrobble", &format!("id={song}&time={}", (now - 3600 * (n as u64 + 1)) * 1000)).await;
    }
    who.ok(reqwest::Method::POST, &format!("/api/track/cratarr/{}/played", tide[0]), json!({})).await;
    for album in [album_id(who, "Far Bank", second).await, album_id(who, "Long Cut", second).await, album_id(who, "Both Shores", music).await] {
        who.ok(reqwest::Method::POST, &format!("/api/albums/cratarr/{album}/favorite?on=true"), json!({})).await;
    }
    for song in [&far[0], &cut[0], &shores[0], &arrived[0], &club[0], &junk[0]] {
        who.ok(reqwest::Method::POST, &format!("/api/track/cratarr/{song}/favorite?on=true"), json!({})).await;
    }
    for (album, stars) in [(album_id(who, "Tide Tables", music).await, 4), (album_id(who, "Partial Shore", music).await, 3)] {
        who.ok(reqwest::Method::POST, &format!("/api/albums/cratarr/{album}/rating?stars={stars}"), json!({})).await;
    }
    for (song, rating) in [(&tide[0], 5), (&cut[0], 2)] {
        ask(who, "setRating", &format!("id={song}&rating={rating}")).await;
    }

    // A list made by an app, and one by the page, a line in each library.
    let lines = [&tide[0], &far[0], &arrived[0], &cut[0], &shores[1]];
    let songs: Vec<String> = lines.iter().map(|s| format!("songId={s}")).collect();
    let by_app = ask(who, "createPlaylist", &format!("name=Both%20rooms&{}", songs.join("&"))).await["playlist"]["id"].as_str().unwrap().to_string();
    let by_page = who.ok(reqwest::Method::POST, "/api/playlists", json!({ "name": "One room" })).await["playlist"].as_str().unwrap().to_string();
    let added = who.ok(reqwest::Method::POST, &format!("/api/playlists/{by_page}/append"), json!({ "source": "cratarr", "ids": lines })).await;
    assert_eq!(added["added"], lines.len(), "a line in every library: {added}");

    let like = json!({ "axis": "like", "not": false, "value": { "kind": "like", "of": [far[0]], "count": 200 } });
    let crate_id = who.ok(reqwest::Method::POST, "/api/crates", json!({ "name": "Like the far bank", "conditions": [like] })).await["id"].as_str().unwrap().to_string();
    who.ok(reqwest::Method::POST, &format!("/api/crates/{crate_id}/left-out"), json!({ "ids": [cut[1]] })).await;

    analysed(who, second, &format!("{FAR}/01 Far Bank.flac"), 40).await;
    // In every library, as which copy plays goes by what they are, and a waveform is the file's that plays.
    analysed(who, music, &format!("{SHORES}/01 Landing.flac"), 80).await;
    analysed(who, second, &format!("{SHORES}/01 Landing.flac"), 80).await;
    analysed(who, incoming, &format!("{SHORES}/01 Landing.mp3"), 80).await;
    analysed(who, incoming, &format!("{ARRIVED}/01 Just Arrived.mp3"), 120).await;
    analysed(who, music, &format!("{TIDE}/01 Slack Water.flac"), 160).await;

    let (two_rooms, tide_album, far_album) = (album_id(who, "Two Rooms", music).await, album_id(who, "Tide Tables", music).await, album_id(who, "Far Bank", second).await);
    Mine {
        playlists: vec![by_app, by_page],
        crate_id,
        heard: heard.iter().map(|s| s.to_string()).collect(),
        shaped: vec![far[0].clone(), shores[0].clone(), arrived[0].clone(), tide[0].clone()],
        covered: vec![tide_album, far_album, two_rooms],
    }
}

/// What the account's rows say, not what any door makes of them: a heart on no list is still held, or still gone.
fn rows_kept(data: &Path) -> Value {
    let rows: serde_json::Map<String, Value> = held(data).into_iter().map(|((who, kind, item), row)| (format!("{who} {kind} {item}"), json!(row))).collect();
    let db = rusqlite::Connection::open(data.join("userdata.sqlite")).unwrap();
    db.busy_timeout(Duration::from_secs(10)).unwrap();
    let mut st = db.prepare("SELECT user, song, at FROM plays ORDER BY at, song").unwrap();
    let plays: Vec<(String, String, i64)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect();
    json!({ "annotation": rows, "plays": plays })
}

/// What `BO` has switched off, each library by what the proof calls it, and which copy of a record in every library that leaves to play.
async fn switched_off(s: &Server, shelves: &Shelves) -> Value {
    let bo = s.sign_in(BO.0, BO.1).await;
    let off = bo.get("/api/me/preferences").await["values"]["hidden_libraries"]["value"].clone();
    let albums = bo.albums().await;
    let shores = albums.iter().find(|a| a["title"] == "Both Shores").expect("a record in every library");
    let plays: Vec<&Value> = shores["copies"].as_array().unwrap().iter().filter(|c| c["plays"] == true).map(|c| &c["library"]).collect();
    shelves.plain(&json!({ "off": off, "plays": plays }))
}

/// Where what `switched_off` finds differs from `want`.
async fn switched_otherwise(s: &Server, shelves: &Shelves, want: &Value) -> Vec<String> {
    let mut out = Vec::new();
    differences("", want, &switched_off(s, shelves).await, &mut out);
    out
}

/// What an account holds of the library, each thing as a door of the API answers it, each library by the name `Shelves` has for it: what
/// a step must leave as it found it. `owned` is what is the listener's, `facts` what the files say.
async fn held_by(who: &Caller<'_>, shelves: &Shelves, mine: &Mine) -> Value {
    const OWNED: [&str; 4] = ["playCount", "played", "starred", "userRating"];
    let mut albums = serde_json::Map::new();
    for a in who.albums().await {
        let id = a["id"].as_str().unwrap().to_string();
        let got = ask(who, "getAlbum", &format!("id={id}")).await["album"].clone();
        let of_song = |s: &Value| json!({ "owned": only(s, &OWNED), "facts": only(s, &["title", "album", "albumId", "artist", "artistId", "track", "discNumber", "year", "genre", "duration", "created", "path"]) });
        let songs: serde_json::Map<String, Value> = got["song"].as_array().into_iter().flatten().map(|s| (s["id"].as_str().unwrap().to_string(), of_song(s))).collect();
        // By name: which of two equal copies comes first goes by where they sort, which is no listener's.
        let mut copies: Vec<Value> = a["copies"].as_array().into_iter().flatten().map(|c| shelves.plain(&only(c, &["library", "format", "added", "plays"]))).collect();
        copies.sort_by_key(|c| c["library"].as_str().unwrap_or("").to_string());
        let facts = only(&got, &["name", "artist", "artistId", "songCount", "duration", "year", "genre", "created"]);
        albums.insert(id, json!({ "owned": only(&got, &OWNED), "facts": facts, "listed": only(&a, &["title", "artist", "year", "added", "tracks", "path"]), "copies": copies, "songs": songs }));
    }
    let starred = ask(who, "getStarred2", "").await["starred2"].clone();
    let favourites = json!({
        "albums": in_order(serde_json::from_value(who.get("/api/favorites/cratarr/albums").await).unwrap()),
        "tracks": shown_ids(&who.send(reqwest::Method::GET, "/api/favorites/cratarr/tracks", Value::Null).await.1),
        "starred albums": ids_in(&starred["album"]),
        "starred songs": ids_in(&starred["song"]),
    });
    let history = who.get("/api/history/listening?period=year").await;
    let top = |list: &str| -> serde_json::Map<String, Value> { history[list].as_array().into_iter().flatten().map(|t| (t["id"].as_str().unwrap().to_string(), only(t, &["name", "by", "plays"]))).collect() };
    let state = who.get("/api/state").await;
    let mut playlists = serde_json::Map::new();
    for id in &mine.playlists {
        let rows = who.get(&format!("/api/playlists/{id}/rows")).await;
        let rows: Vec<Value> = rows["rows"].as_array().unwrap().iter().map(|r| only(r, &["entry_id", "id", "name", "artist", "album", "album_id", "track", "missing", "play_count", "last_played", "added"])).collect();
        let listed = state["playlists"].as_array().unwrap().iter().find(|p| p["id"] == id.as_str()).map(|p| only(p, &["name", "count", "dead"]));
        playlists.insert(id.clone(), json!({ "listed": listed, "rows": rows }));
    }
    let id = &mine.crate_id;
    let left_out = who.get(&format!("/api/crates/{id}/left-out")).await;
    let left_out: Vec<Value> = left_out["rows"].as_array().unwrap().iter().map(|r| only(r, &["id", "name", "artist"])).collect();
    let def = who.get(&format!("/api/crates/{id}")).await;
    let the_crate = json!({ "def": only(&def, &["name", "conditions", "names"]), "rows": shown_ids(&who.send(reqwest::Method::GET, &format!("/api/crates/{id}/rows"), Value::Null).await.1), "left_out": left_out });
    let mut waveforms = serde_json::Map::new();
    for id in &mine.shaped {
        let (status, bins) = who.bytes(&format!("/api/track/cratarr/{id}/waveform")).await;
        waveforms.insert(id.clone(), json!({ "status": status, "bins": print_of(&bins) }));
    }
    let mut sounds = serde_json::Map::new();
    for id in &mine.heard {
        let (status, file) = who.bytes(&format!("/api/file/cratarr/{id}")).await;
        sounds.insert(id.clone(), if status == 200 { json!(sound_of(&file)) } else { json!(status) });
    }
    let mut covers = serde_json::Map::new();
    for id in &mine.covered {
        let (status, picture) = who.bytes(&format!("/api/albums/cratarr/{id}/cover")).await;
        covers.insert(id.clone(), json!({ "status": status, "picture": print_of(&picture) }));
    }
    // An app keeps a library by its number (`musicFolderId`), and a listing says it beside the folder's name as it stands now.
    let numbers = ask(who, "getMusicFolders", "").await;
    let by_name = |f: &Value| shelves.list.iter().find(|s| s.at.file_name().and_then(|n| n.to_str()) == f["name"].as_str()).map_or(f["name"].to_string(), |s| s.name.to_string());
    let folders: serde_json::Map<String, Value> = numbers["musicFolders"]["musicFolder"].as_array().into_iter().flatten().map(|f| (by_name(f), f["id"].clone())).collect();
    let catalogue = who.get("/api/catalogue").await;
    shelves.plain(&json!({
        "folders": folders,
        "albums": albums,
        "favourites": favourites,
        "uses": who.get("/api/album-uses/cratarr").await,
        "history": { "plays": history["plays"], "artists": top("artists"), "albums": top("albums"), "tracks": top("tracks") },
        "playlists": playlists,
        "crate": the_crate,
        "waveforms": waveforms,
        "sounds": sounds,
        "covers": covers,
        "rows": rows_kept(&shelves.data),
        // What says something went wrong: nothing new may turn up in any of these.
        "moved": who.get("/api/admin/migration/moved").await,
        "report": who.get("/api/admin/migration").await,
        "repairs": who.get("/api/repairs").await,
        "catalogue": only(&catalogue, &["tags_failed", "issues"]),
        "problems": catalogue["progress"]["problems"],
    }))
}

/// Where two answers differ, each place by its path: the snapshot's helper, as a whole tree in an assert says nothing.
fn differences(at: &str, want: &Value, got: &Value, out: &mut Vec<String>) {
    match (want, got) {
        (Value::Object(w), Value::Object(g)) => {
            for key in w.keys().chain(g.keys().filter(|k| !w.contains_key(*k))) {
                differences(&format!("{at}/{key}"), w.get(key).unwrap_or(&Value::Null), g.get(key).unwrap_or(&Value::Null), out);
            }
        }
        (Value::Array(w), Value::Array(g)) if w.len() == g.len() => {
            for (n, (w, g)) in w.iter().zip(g).enumerate() {
                differences(&format!("{at}/{n}"), w, g, out);
            }
        }
        _ if want != got => out.push(format!("{at}: {want} became {got}")),
        _ => {}
    }
}

/// What still differs by `differ` in what `who` holds, once a start or a change has had its time: nothing, or what never came.
async fn settled(who: &Caller<'_>, shelves: &Shelves, mine: &Mine, differ: impl Fn(&Value) -> Vec<String>) -> Vec<String> {
    let start = Instant::now();
    loop {
        built(who).await;
        let left = differ(&held_by(who, shelves, mine).await);
        if left.is_empty() || start.elapsed() > SETTLES {
            return left;
        }
        // A pass ending in the second of the last look goes unseen (`Serve::refresh`), and a change outside Cratarr waits for a pass.
        ask(who, "startScan", "").await;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// `want`, against what `settled` read.
fn from(want: &Value) -> impl Fn(&Value) -> Vec<String> + '_ {
    |got| {
        let mut out = Vec::new();
        differences("", want, got, &mut out);
        out
    }
}

/// `from`, for what was only just saved or seen: a list's counts are read again at the minute's pass, as files come and go with no
/// playlist changing (`refresh_dead`), so they are up to a minute behind a library coming back. A start reads them at once, and the
/// compare after one has them.
fn from_at_once(want: &Value) -> impl Fn(&Value) -> Vec<String> {
    let but_the_counts = |held: &Value| {
        let mut held = held.clone();
        held["playlists"].as_object_mut().unwrap().values_mut().for_each(|p| drop(p.as_object_mut().unwrap().remove("listed")));
        held
    };
    let want = but_the_counts(want);
    move |got| {
        let mut out = Vec::new();
        differences("", &want, &but_the_counts(got), &mut out);
        out
    }
}

/// What must stand while a library is away: every row of the account's, what each album left has by its id, each list's lines and the
/// crate's own. Not what a door makes of it, as a record gone from every list has nothing to show its play on. `gone`: the albums lying
/// only there. `lacking`: the tracks lying only there of a record that stays, by a copy short of them.
fn kept_while_away(held: &Value, gone: &[String], lacking: &[String]) -> Value {
    let album = |a: &Value| {
        let songs: serde_json::Map<String, Value> = a["songs"].as_object().unwrap().iter().filter(|(id, _)| !lacking.contains(id)).map(|(id, s)| (id.clone(), s["owned"].clone())).collect();
        json!({ "owned": a["owned"], "songs": songs })
    };
    let albums: serde_json::Map<String, Value> = held["albums"].as_object().unwrap().iter().filter(|(id, _)| !gone.contains(id)).map(|(id, a)| (id.clone(), album(a))).collect();
    let lines = |p: &Value| -> Vec<Value> { p["rows"].as_array().unwrap().iter().map(|r| only(r, &["entry_id", "name"])).collect() };
    let playlists: serde_json::Map<String, Value> = held["playlists"].as_object().unwrap().iter().map(|(id, p)| (id.clone(), json!(lines(p)))).collect();
    json!({ "albums": albums, "playlists": playlists, "conditions": held["crate"]["def"]["conditions"], "left_out": held["crate"]["left_out"], "rows": held["rows"], "moved": held["moved"], "report": held["report"] })
}

/// The albums of `want` lying in `library` alone.
fn lying_only_in(want: &Value, library: &str) -> Vec<String> {
    let there = format!("<{library}>");
    let only_there = |a: &Value| a["copies"].as_array().unwrap().iter().all(|c| c["library"] == there.as_str());
    want["albums"].as_object().unwrap().iter().filter(|(_, a)| only_there(a)).map(|(id, _)| id.clone()).collect()
}

/// What differs from `want` while `library` is away, by `kept_while_away`, and a line of a record gone that does not say it is missing.
fn from_while_away<'a>(want: &'a Value, library: &'a str, lacking: &'a [String]) -> impl Fn(&Value) -> Vec<String> + 'a {
    move |got| {
        let gone = lying_only_in(want, library);
        let mut out = Vec::new();
        differences("", &kept_while_away(want, &gone, lacking), &kept_while_away(got, &[], &[]), &mut out);
        for (id, list) in want["playlists"].as_object().unwrap() {
            for (n, row) in list["rows"].as_array().unwrap().iter().enumerate() {
                let is_gone = gone.iter().any(|a| row["album_id"] == a.as_str());
                if got["playlists"][id]["rows"][n]["missing"] != is_gone {
                    out.push(format!("{id}: line {n}, {}, missing is not {is_gone}: {}", row["name"], got["playlists"][id]["rows"][n]));
                }
            }
        }
        out
    }
}

/// `want` taking what `got` has at `at`, a thing a step brought: named one by one, so nothing else slips in with it.
fn taking(want: &mut Value, got: &Value, at: &str) {
    let (parent, key) = at.rsplit_once('/').unwrap();
    want.pointer_mut(parent).unwrap().as_object_mut().unwrap().insert(key.to_string(), got.pointer(at).cloned().unwrap_or(Value::Null));
}

/// `taking`, where all that was there must still be.
fn growing(want: &mut Value, got: &Value, at: &str, findings: &mut Vec<String>) {
    let kept = match (want.pointer(at), got.pointer(at)) {
        (Some(Value::Array(was)), Some(Value::Array(is))) => was.iter().all(|x| is.contains(x)),
        (Some(Value::Object(was)), Some(Value::Object(is))) => was.iter().all(|(k, v)| is.get(k) == Some(v)),
        _ => false,
    };
    if kept {
        taking(want, got, at);
    } else {
        findings.push(format!("{at}: {} is not all still in {}", want.pointer(at).unwrap_or(&Value::Null), got.pointer(at).unwrap_or(&Value::Null)));
    }
}

/// `held` with text as a change outside Cratarr left it, the same wherever it is said.
fn retold(held: &Value, was: &str, is: &str) -> Value {
    serde_json::from_str(&held.to_string().replace(was, is)).unwrap()
}

/// The server stopped and started again on what it has.
async fn restarted(s: Server, data: &Path) -> Server {
    s.stop();
    Server::start(data).await
}

fn say(findings: &mut Vec<String>, step: &str, left: Vec<String>) {
    findings.extend(left.into_iter().map(|l| format!("{step}: {l}")));
}

/// A library with a record of its own, from a file copied in and tagged as nothing there is.
fn a_new_library(at: &Path, title: &str, from: &Path) {
    let dir = at.join("Salt Meridian").join(format!("{title} (2018)"));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("01 {title}.flac"));
    std::fs::copy(from, &file).unwrap();
    set_vorbis(&file, "ALBUM", title);
    set_vorbis(&file, "TITLE", title);
}

/// The whole chain for a listener (the owner, 2026-11-06: "importing/resyncing libraries while preserving database integrity is
/// important"): on 0.15, adding a library shifted folder numbers, and hearts, plays and waveforms went out of view. An account's listening on
/// two main libraries and an Incoming one, through the real binary on real files, read at every door as a person's page and apps read
/// it; then each thing that moves a library, through Settings and a restart, as a person would. After each, every id is what it was, and
/// everything held points at the same music. Every step is gone through whatever one before it found, so one run says all there is.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_library_changing_place_changes_nothing_a_listener_owns_end_to_end() {
    let mut shelves = Shelves::made("resync");
    let mut findings: Vec<String> = Vec::new();
    let s = Server::start(&shelves.data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.ok(reqwest::Method::POST, "/api/users", json!({ "name": ANN.0, "password": ANN.1, "admin": true })).await;
    let ann = s.sign_in(ANN.0, ANN.1).await;
    let copies_of = |a: &Value, n: usize| a["title"] == "Both Shores" && a["copies"].as_array().is_some_and(|c| c.len() == n);
    s.until("every library read", Duration::from_secs(180), || async {
        built(&ann).await;
        let albums = ann.albums().await;
        ["Far Bank", "Just Arrived", "Tide Tables"].iter().all(|t| albums.iter().any(|a| a["title"] == *t)) && albums.iter().any(|a| copies_of(a, 3))
    })
    .await;
    let mine = listen(&ann, &shelves).await;
    let mut want = held_by(&ann, &shelves, &mine).await;
    let shores = album_id(&ann, "Both Shores", shelves.at("music")).await;
    // What the proof stands on: all of it there to lose.
    assert!(want["sounds"].as_object().unwrap().values().all(Value::is_string), "every play has a file to hear: {}", want["sounds"]);
    assert!(want["waveforms"].as_object().unwrap().values().all(|w| w["status"] == 200), "{}", want["waveforms"]);
    assert!(want["covers"].as_object().unwrap().values().all(|c| c["status"] == 200), "{}", want["covers"]);
    assert_eq!(want["crate"]["left_out"].as_array().unwrap().len(), 1, "{}", want["crate"]);
    assert!(want["playlists"].as_object().unwrap().values().all(|p| p["listed"]["count"] == 5 && p["listed"]["dead"] == 0), "{}", want["playlists"]);

    // Nothing changed: a restart alone moves nothing, or no step below could say what moved it.
    let s = restarted(s, &shelves.data).await;
    let ann = s.sign_in(ANN.0, ANN.1).await;
    say(&mut findings, "a restart", settled(&ann, &shelves, &mine, from(&want)).await);

    // 1. A library that sorts before the others, as a capital does before every small letter, added in Settings: Incoming's folders are
    // sorted, so production's new library went in front of the one there. It brings an album of its own and a copy of a record held.
    let landed = shelves.root.join("library/Landed");
    let far = shelves.at("second").join(FAR).join("01 Far Bank.flac");
    a_new_library(&landed, "First Light", &far);
    let (from_shores, to_shores) = (shelves.at("incoming").join(SHORES), landed.join(SHORES));
    std::fs::create_dir_all(&to_shores).unwrap();
    for file in audio_under(&from_shores) {
        std::fs::copy(from_shores.join(&file), to_shores.join(&file)).unwrap();
    }
    shelves.list.push(Shelf { at: landed.clone(), kind: "incoming", name: "landed" });
    let s = shelves.saved(s).await;
    let ann = s.sign_in(ANN.0, ANN.1).await;
    s.until("the new library read", SETTLES, || async {
        built(&ann).await;
        ask(&ann, "startScan", "").await;
        ann.albums().await.iter().any(|a| a["title"] == "First Light")
    })
    .await;
    let first_light = album_id(&ann, "First Light", &landed).await;
    let got = held_by(&ann, &shelves, &mine).await;
    // What was added is what changes: the library with a number of its own, the new album, the copy on its record, what the copy's new
    // files were taken by, and a crate that draws on every library.
    taking(&mut want, &got, "/folders/landed");
    taking(&mut want, &got, &format!("/albums/{first_light}"));
    taking(&mut want, &got, &format!("/albums/{shores}/copies"));
    growing(&mut want, &got, "/moved/tracks", &mut findings);
    growing(&mut want, &got, "/crate/rows", &mut findings);
    say(&mut findings, "a library added", settled(&ann, &shelves, &mine, from_at_once(&want)).await);
    let s = restarted(s, &shelves.data).await;
    let ann = s.sign_in(ANN.0, ANN.1).await;
    say(&mut findings, "a library added, started again", settled(&ann, &shelves, &mine, from(&want)).await);

    // A main library put first in the list, as the page can: it takes every fallback from now on, and nothing that was held goes with it.
    let annex = shelves.root.join("library/Annex");
    a_new_library(&annex, "Annexed", &shelves.at("second").join(FAR).join("01 Far Bank.flac"));
    shelves.list.insert(0, Shelf { at: annex.clone(), kind: "main", name: "annex" });
    let s = shelves.saved(s).await;
    let ann = s.sign_in(ANN.0, ANN.1).await;
    s.until("the new main library read", SETTLES, || async {
        built(&ann).await;
        ask(&ann, "startScan", "").await;
        ann.albums().await.iter().any(|a| a["title"] == "Annexed")
    })
    .await;
    let annexed = album_id(&ann, "Annexed", &annex).await;
    let got = held_by(&ann, &shelves, &mine).await;
    taking(&mut want, &got, "/folders/annex");
    taking(&mut want, &got, &format!("/albums/{annexed}"));
    growing(&mut want, &got, "/crate/rows", &mut findings);
    say(&mut findings, "a main library put first", settled(&ann, &shelves, &mine, from_at_once(&want)).await);
    let s = restarted(s, &shelves.data).await;
    let ann = s.sign_in(ANN.0, ANN.1).await;
    say(&mut findings, "a main library put first, started again", settled(&ann, &shelves, &mine, from(&want)).await);

    // A library switched off stays off wherever it goes, and a switch says its library's path: a rename that did not reach it would turn
    // the library back on, for one account and on every device. Both main libraries that hold the record, so a switch lost plays that one.
    ann.ok(reqwest::Method::POST, "/api/users", json!({ "name": BO.0, "password": BO.1 })).await;
    let off: Vec<String> = ["music", "second", "landed"].iter().map(|name| format!("cratarr/{}", shelves.at(name).display())).collect();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
    s.sign_in(BO.0, BO.1).await.ok(reqwest::Method::PUT, "/api/me/preferences/hidden_libraries", json!({ "value": off, "changed": now })).await;
    let bos = switched_off(&s, &shelves).await;
    assert_eq!(bos["off"], json!(["cratarr/<music>", "cratarr/<second>", "cratarr/<landed>"]), "{bos}");

    // 2. A library's folder renamed, mount and settings together, with the server down, as a mount takes a restart. By a new name, and
    // only by case, which turned production's two Incoming folders round, and the first main library's too.
    let mut s = s;
    for (name, to) in [("second", "shelf"), ("landed", "landed"), ("music", "Music")] {
        s.stop();
        shelves.rename(name, to);
        shelves.write();
        s = Server::start(&shelves.data).await;
        let ann = s.sign_in(ANN.0, ANN.1).await;
        say(&mut findings, &format!("{name} renamed {to}"), settled(&ann, &shelves, &mine, from(&want)).await);
        say(&mut findings, &format!("{name} renamed {to}, its switch"), switched_otherwise(&s, &shelves, &bos).await);
    }

    // In two moves, the way a mount is changed first: it starts with the library not there, and Settings is told afterwards.
    s.stop();
    shelves.rename("second", "rack");
    s = Server::start(&shelves.data).await;
    s = shelves.saved(s).await;
    let ann = s.sign_in(ANN.0, ANN.1).await;
    say(&mut findings, "second renamed rack, then told", settled(&ann, &shelves, &mine, from_at_once(&want)).await);
    say(&mut findings, "second renamed rack, then told, its switch", switched_otherwise(&s, &shelves, &bos).await);

    // 3. A library removed, the server restarted, and the library added back. While it is away what lies only there is off every list and
    // its lines read as missing, as nobody could hear it; nothing it holds is touched. Each kind of library, and the first of all. Incoming's
    // copy of Short Measure is short of its last track: with Music away the record stays by that copy, and that track goes with its file.
    let (whole, short) = (tracks_at(&shelves.data, shelves.at("music"), MEASURE), tracks_at(&shelves.data, shelves.at("incoming"), MEASURE));
    let only_in_music: Vec<String> = whole.into_iter().filter(|t| !short.contains(t)).collect();
    assert_eq!(only_in_music.len(), 1, "{short:?}");
    for name in ["second", "incoming", "music"] {
        let lacking: &[String] = if name == "music" { &only_in_music } else { &[] };
        let at = shelves.list.iter().position(|l| l.name == name).unwrap();
        let shelf = shelves.list.remove(at);
        s = shelves.saved(s).await;
        let ann = s.sign_in(ANN.0, ANN.1).await;
        say(&mut findings, &format!("{name} removed"), settled(&ann, &shelves, &mine, from_while_away(&want, name, lacking)).await);
        s = restarted(s, &shelves.data).await;
        let ann = s.sign_in(ANN.0, ANN.1).await;
        say(&mut findings, &format!("{name} removed, started again"), settled(&ann, &shelves, &mine, from_while_away(&want, name, lacking)).await);
        shelves.list.insert(at, shelf);
        s = shelves.saved(s).await;
        let ann = s.sign_in(ANN.0, ANN.1).await;
        say(&mut findings, &format!("{name} added back"), settled(&ann, &shelves, &mine, from_at_once(&want)).await);
        s = restarted(s, &shelves.data).await;
        let ann = s.sign_in(ANN.0, ANN.1).await;
        say(&mut findings, &format!("{name} added back, started again"), settled(&ann, &shelves, &mine, from(&want)).await);
        say(&mut findings, &format!("{name} added back, its switch"), switched_otherwise(&s, &shelves, &bos).await);
    }

    // 4. Files changed outside Cratarr, as a tagger and a file manager do, with the server running.
    let ann = s.sign_in(ANN.0, ANN.1).await;
    let (music, second) = (shelves.at("music").to_path_buf(), shelves.at("second").to_path_buf());
    // Retagged where they lie: a track's title, and an album's on all its tracks. What the files say is what shows, so those words change
    // wherever they are shown. The track's waveform goes out of view until the pass has been over it: an analysis is of the file as it
    // was, and nothing here says the sound is what it was.
    set_vorbis(&music.join(TIDE).join("01 Slack Water.flac"), "TITLE", "Slack Water Retold");
    for file in audio_under(&music.join(PARTIAL)) {
        set_vorbis(&music.join(PARTIAL).join(file), "ALBUM", "Partial Shore Retold");
    }
    want = retold(&want, "\"Slack Water\"", "\"Slack Water Retold\"");
    want = retold(&want, "\"Partial Shore\"", "\"Partial Shore Retold\"");
    want["waveforms"][mine.shaped[3].as_str()] = json!({ "status": 404, "bins": print_of(&[]) });
    // Renamed, the tags as they were, and a title changed with the name at once, which its recording tells: their paths say so, and that
    // title. Not asked here: both at once with no recording. Tags alone never pair two files, so that is a new id rather than a wrong one
    // (`identity`, decision 0011).
    std::fs::rename(music.join(CLUB).join("01 One.flac"), music.join(CLUB).join("A1 One.flac")).unwrap();
    want = retold(&want, &format!("<music>/{CLUB}/01 One.flac"), &format!("<music>/{CLUB}/A1 One.flac"));
    set_vorbis(&music.join(JUNK).join("01 Zero.flac"), "TITLE", "Nought");
    std::fs::rename(music.join(JUNK).join("01 Zero.flac"), music.join(JUNK).join("01 Nought.flac")).unwrap();
    want = retold(&want, "\"Zero\"", "\"Nought\"");
    want = retold(&want, &format!("<music>/{JUNK}/01 Zero.flac"), &format!("<music>/{JUNK}/01 Nought.flac"));
    // A whole album's folder moved inside its library, one lying there alone and a copy of a record: its paths say so, and nothing else.
    std::fs::create_dir_all(second.join("Moved")).unwrap();
    std::fs::rename(second.join(FAR), second.join("Moved/Far Bank")).unwrap();
    want = retold(&want, &format!("<second>/{FAR}"), "<second>/Moved/Far Bank");
    std::fs::create_dir_all(music.join("Moved")).unwrap();
    std::fs::rename(music.join(SHORES), music.join("Moved/Both Shores")).unwrap();
    want = retold(&want, &format!("<music>/{SHORES}"), "<music>/Moved/Both Shores");
    ask(&ann, "startScan", "").await;
    say(&mut findings, "files changed outside", settled(&ann, &shelves, &mine, from_at_once(&want)).await);
    let s = restarted(s, &shelves.data).await;
    let ann = s.sign_in(ANN.0, ANN.1).await;
    say(&mut findings, "files changed outside, started again", settled(&ann, &shelves, &mine, from(&want)).await);

    assert!(findings.is_empty(), "{} found:\n{}", findings.len(), findings.join("\n"));
    s.stop();
    std::fs::remove_dir_all(&shelves.root).ok();
}
