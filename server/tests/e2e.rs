//! The end-to-end suite (decision 0093): the real server binary on a fresh copy of the fixture library and a fresh data
//! folder, driven through its API as the page drives it. Types into the files (decision 0092): the catalogue read, the
//! review sheet, a run written and read back from the files, "changed since it was planned", Undo, and the run kept
//! across a restart. Everything is thrown away after. The Dockerfile's test and `e2e` stages run them by name, so a test
//! here ends its name in `end_to_end` or the gate never runs it: `cargo test --workspace -- --ignored fixtures:: end_to_end`.
//! What the records' rewrite must not break has a test of its own, each with its Undo: Keep, Delete, Replace, a length's fix
//! and a cover, and a second run waiting for the one writing.
//!
//! `Server` calls as nobody, which a server with no accounts takes for its administrator. `Server::sign_in` gives a
//! `Caller` that is an account, `settings` writes 0.16's settings for the whole layout (`--make-layout`), `settings_0_15`
//! what 0.15 left.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
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

/// A data folder as 0.15 left it over `--make-layout`'s files (`tests/fixtures/0.15`, written down from tag v0.15.0, as 0093
/// keeps no binaries in git): its catalogue, so its track ids mean what its user data says they mean; its user data under
/// 0.15's ids; each account's crates, playlists and settings. 0.15 had one Music folder, so the layout's second library is an
/// Incoming there, and own music did not exist. The folders and each file's size and time are this run's: as they stand in
/// the text, every file would read as changed.
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
    for table in ["files", "cover_files"] {
        let rows: Vec<(i64, String)> = db.prepare(&format!("SELECT folder, rel FROM {table}")).unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect();
        for (folder, rel) in rows {
            let meta = std::fs::metadata(folders[folder as usize].join(&rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
            let mtime = meta.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
            db.execute(&format!("UPDATE {table} SET size = ?1, mtime = ?2 WHERE folder = ?3 AND rel = ?4"), rusqlite::params![meta.len() as i64, mtime, folder, rel]).unwrap();
        }
    }
    drop(db);
    rusqlite::Connection::open(data.join("userdata.sqlite")).unwrap().execute_batch(&text("userdata.sql")).unwrap();
    std::fs::create_dir_all(data.join("users/bob")).unwrap();
    for file in ["crates.json", "users/bob/crates.json", "subsonic-users.json"] {
        std::fs::copy(here.join(file), data.join(file)).unwrap();
    }
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

/// A crate of the fixture's Undated, four tracks of one genre, on a server of its own: (server, root, the crate, its tracks).
async fn a_crate_of_four(name: &str) -> (Server, PathBuf, String, Vec<String>) {
    let root = temp(name);
    let (music, data) = (root.join("music"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    assert!(Command::new(BIN).arg("--make-fixtures").arg(&music).status().unwrap().success(), "the fixture library is made");
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

/// An artist's id for life on a running server: what the library serves, the panel's heart by name, an artist seed's crate
/// by id, and all of it the same after a restart. A heart on a name no file credits yet is kept too, as it was under the hash.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn an_artist_is_one_id_by_name_and_by_number_end_to_end() {
    let root = temp("artist-id");
    let (music, data) = (root.join("music"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    assert!(Command::new(BIN).arg("--make-fixtures").arg(&music).status().unwrap().success(), "the fixture library is made");
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

/// What the migration's run stands on (decision 0101 point 28): 0.15's data opens on what is built now, its files found as
/// they were left and nothing taken for new. A fixture whose rows did not fit its files would give them fresh tracks, and
/// `tr-<n>` in the user data would point at nothing, so the proof on it would prove nothing (`ready` E10).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn the_data_of_0_15_opens_on_its_own_tracks_end_to_end() {
    let root = temp("data-0-15");
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    assert!(Command::new(BIN).arg("--make-layout").arg(&library).status().unwrap().success(), "the whole layout is made");
    data_0_15(&library, &data);
    let tracks = |db: &rusqlite::Connection| -> Vec<(i64, String, i64)> {
        db.prepare("SELECT folder, rel, track FROM files ORDER BY folder, rel").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect()
    };
    let left = tracks(&rusqlite::Connection::open(data.join("catalogue.sqlite")).unwrap());
    assert_eq!(left.len(), audio_under(&library.join("music")).len() + audio_under(&library.join("incoming")).len() + audio_under(&library.join("second")).len(), "a row for every file");

    let s = Server::start(&data).await;
    let ann = s.sign_in("ann", "ann's password").await;
    ann.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    // Every library read: the albums' titles, 20 of the layout's 22, as its two accounts' own have no place in 0.15.
    let titles = |albums: Vec<Value>| albums.iter().filter_map(|a| a["title"].as_str().map(str::to_string)).collect::<std::collections::BTreeSet<String>>();
    s.until("every library read", Duration::from_secs(180), || async { titles(ann.albums().await).len() == 20 }).await;

    // Its tracks are the ones it left, every file on the id its user data names.
    let db = rusqlite::Connection::open(data.join("catalogue.sqlite")).unwrap();
    assert_eq!(tracks(&db), left, "nothing new, nothing moved");
    assert_eq!(db.query_row("SELECT count(*) FROM tracks", [], |r| r.get::<_, i64>(0)).unwrap(), left.len() as i64 + 1, "and the retired one");
    let tide = ann.albums().await.into_iter().find(|a| a["title"] == "Tide Tables").unwrap();
    let rows = ann.get(&format!("/api/albums/cratarr/{}", tide["id"].as_str().unwrap())).await;
    let slack = left.iter().find(|(_, rel, _)| rel.ends_with("Slack Water.flac")).unwrap().2;
    assert!(rows["rows"].as_array().unwrap().iter().any(|r| r["id"] == format!("tr-{slack}") && r["name"] == "Slack Water"), "{rows}");

    // What is not a table came with it: the settings' roles, each account's crates, the playlists with their gone line.
    let report = ann.get("/api/admin/migration").await;
    assert_eq!(report["parts"]["libraries"]["sources"]["cratarr"], json!({ "main": 1, "incoming": 2 }));
    assert_eq!((report["parts"]["playlists"]["playlists"].as_u64(), report["parts"]["playlists"]["lines"].as_u64()), (Some(2), Some(4)), "{report}");
    let crates = ann.get("/api/crates").await;
    assert!(crates.as_array().unwrap().iter().any(|c| c["name"] == "Like both shores"), "{crates}");
    let bob = s.sign_in("bob", "bob's password").await;
    let crates = bob.get("/api/crates").await;
    assert!(crates.as_array().unwrap().iter().any(|c| c["name"] == "Like the rooms"), "{crates}");
    let state = ann.get("/api/state").await;
    let mix = state["playlists"].as_array().unwrap().iter().find(|p| p["name"] == "Mix").cloned().expect("Mix brought in");
    let lines = ann.get(&format!("/api/playlists/{}/rows", mix["id"].as_str().unwrap())).await;
    assert_eq!(lines["rows"].as_array().unwrap().iter().map(|r| r["missing"].as_bool().unwrap()).collect::<Vec<_>>(), [false, false, true]);
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
    assert_eq!(in_order[0], path(&own.join("ann")), "her own music first, though it is an MP3");
    assert_eq!(in_order[3], path(&incoming), "Incoming last");
    assert_eq!((copies(&shores[0])[0]["plays"].as_bool(), shores[0]["format"].as_str(), shores[0]["library"].as_str()), (Some(true), Some("MP3"), Some(&*in_order[0])), "the card is what plays");
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
    assert_eq!(copies(&papers[0])[0]["library"].as_str(), Some(&*path(&own.join("bob"))), "his own music, at a lower quality");
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
    assert!(Command::new(BIN).arg("--make-layout").arg(&library).status().unwrap().success(), "the whole layout is made");
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
    let made = Command::new(BIN).arg("--make-fixtures").arg(&music).status().unwrap();
    assert!(made.success(), "the fixture library is made");
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
    let made = Command::new(BIN).arg("--make-fixtures").arg(&music).status().unwrap();
    assert!(made.success(), "the fixture library is made");
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
    assert!(Command::new(BIN).arg("--make-fixtures").arg(&music).status().unwrap().success(), "the fixture library is made");
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
    assert!(Command::new(BIN).arg("--make-layout").arg(&library).status().unwrap().success(), "the whole layout is made");
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
    assert!(Command::new(BIN).arg("--make-layout").arg(&library).status().unwrap().success(), "the whole layout is made");
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
    assert!(Command::new(BIN).arg("--make-layout").arg(&library).status().unwrap().success(), "the whole layout is made");
    let (music, incoming) = (library.join("music"), library.join("incoming"));
    // No own music: the record would play from ann's copy.
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
    assert!(Command::new(BIN).arg("--make-layout").arg(&library).status().unwrap().success(), "the whole layout is made");
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
    assert!(Command::new(BIN).arg("--make-layout").arg(&library).status().unwrap().success(), "the whole layout is made");
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

/// "Plays from", through the routes: a pick changes what plays for that account alone, and holds until its copy goes. An Undo
/// brings the copy back, not the pick (Q13).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn a_pick_holds_for_its_account_until_its_copy_goes_end_to_end() {
    let root = temp("pick");
    let (library, data) = (root.join("library"), root.join("data"));
    std::fs::create_dir_all(&data).unwrap();
    assert!(Command::new(BIN).arg("--make-layout").arg(&library).status().unwrap().success(), "the whole layout is made");
    let (music, incoming) = (library.join("music"), library.join("incoming"));
    settings(&data, &[(&music, "main"), (&incoming, "incoming")], &root.join("no-own-music"));
    let s = Server::start(&data).await;
    s.ok(reqwest::Method::PUT, "/api/settings/musicbrainz", json!({ "enabled": false })).await;
    s.ok(reqwest::Method::POST, "/api/users", json!({ "name": "ann", "password": "ann's password", "admin": true })).await;
    let ann = s.sign_in("ann", "ann's password").await;
    ann.ok(reqwest::Method::POST, "/api/users", json!({ "name": "bob", "password": "bob's password" })).await;
    let bob = s.sign_in("bob", "bob's password").await;
    let path = |p: &Path| p.to_string_lossy().to_string();
    let both = |a: &Value| a["title"] == "Both Shores" && a["copies"].as_array().is_some_and(|c| c.len() == 2);
    s.until("both copies read", Duration::from_secs(120), || async { ann.albums().await.iter().any(both) }).await;
    let shores = |list: Vec<Value>| list.into_iter().find(|a| a["title"] == "Both Shores").unwrap();
    let id = shores(ann.albums().await)["id"].as_str().unwrap().to_string();
    let picking = format!("/api/albums/cratarr/{id}/copy-pick");
    let copy = |a: &Value, p: &Path| a["copies"].as_array().unwrap().iter().find(|c| c["library"].as_str() == Some(&*path(p))).cloned().unwrap();

    assert_eq!(ann.send(reqwest::Method::PUT, &picking, json!({ "library": path(&root) })).await.0, 409, "no copy there");
    assert_eq!(s.anyone().send(reqwest::Method::PUT, &picking, json!({ "library": path(&incoming) })).await.0, 401);
    ann.ok(reqwest::Method::PUT, &picking, json!({ "library": path(&incoming) })).await;

    let theirs = shores(ann.albums().await);
    assert_eq!((copy(&theirs, &incoming)["picked"].as_bool(), copy(&theirs, &incoming)["plays"].as_bool()), (Some(true), Some(true)), "{theirs}");
    assert_eq!((copy(&theirs, &music)["picked"].as_bool(), copy(&theirs, &music)["plays"].as_bool()), (Some(false), Some(false)));
    assert_eq!((theirs["format"].as_str(), theirs["library"].as_str()), (Some("MP3"), Some(&*path(&incoming))), "the card is what plays for her");
    assert_eq!(theirs["copies"][0]["library"].as_str(), Some(&*path(&incoming)), "first in her order");
    let his = shores(bob.albums().await);
    assert_eq!((copy(&his, &incoming)["picked"].as_bool(), copy(&his, &music)["plays"].as_bool(), his["format"].as_str()), (Some(false), Some(true), Some("FLAC")), "nothing changes for bob");

    // What is heard, not only what is listed: one track id, a file of its own for each account.
    let dir = "Salt Meridian/Both Shores (2018)";
    let song = tracks_at(&root.join("data"), &music, dir).remove(0);
    let (flac, mp3) = (std::fs::read(music.join(dir).join("01 Landing.flac")).unwrap(), std::fs::read(incoming.join(dir).join("01 Landing.mp3")).unwrap());
    assert_eq!(tracks_at(&root.join("data"), &incoming, dir)[0], song, "the copies share it");
    let file = format!("/api/file/cratarr/{song}");
    assert!(ann.bytes(&file).await.1 == mp3, "her pick");
    assert!(bob.bytes(&file).await.1 == flac, "his, the kept copy");

    let gone = ann.ok(reqwest::Method::POST, &format!("/api/albums/cratarr/{id}/delete"), json!({ "from": path(&incoming) })).await;
    let user = rusqlite::Connection::open(data.join("userdata.sqlite")).unwrap();
    let left: i64 = user.query_row("SELECT count(*) FROM copy_picks", [], |r| r.get(0)).unwrap();
    assert_eq!(left, 0, "that copy went");
    assert!(ann.bytes(&file).await.1 == flac, "with her copy gone she hears the kept one");
    ann.ok(reqwest::Method::POST, "/api/undo", json!({ "tokens": [gone["undo"]] })).await;
    s.until("the copy back", Duration::from_secs(120), || async { ann.albums().await.iter().any(both) }).await;
    let back = shores(ann.albums().await);
    assert_eq!((copy(&back, &incoming)["picked"].as_bool(), copy(&back, &music)["plays"].as_bool()), (Some(false), Some(true)), "the copy is back, the pick is not");
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
    assert!(Command::new(BIN).arg("--make-layout").arg(&library).status().unwrap().success(), "the whole layout is made");
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
    assert!(Command::new(BIN).arg("--make-layout").arg(&library).status().unwrap().success(), "the whole layout is made");
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
    assert!(Command::new(BIN).arg("--make-layout").arg(&library).status().unwrap().success(), "the whole layout is made");
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
    assert!(Command::new(BIN).arg("--make-layout").arg(&library).status().unwrap().success(), "the whole layout is made");
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
