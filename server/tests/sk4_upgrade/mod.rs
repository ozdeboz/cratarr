//! SK4.7 (SCENARIOS-SK17, Part 4): 0.16.3's Soulseek queue through 0.17's first start, on the real binary.
//!
//! The data folder as 0.16.3 leaves it: FX01's tables (`tests/fixtures/0.16`, as the suite's `data_0_16` makes them: 0.16.3 changed no
//! schema), the queue fixture's requests (`tests/fixtures/0.16.3/requests.sql`) in its user data, and its queue (`soulseek.json`)
//! over FX01's; slskd set as 0.16.3 kept it, among the server's choices. slskd (`Slskd0163`) holds 0.16.3's two downloads, serves each
//! batch its own files, takes one operation at a time and finds nothing in a search. The connections 0.16.3 can switch off are off,
//! as the suite keeps them (decision 0093). FX01 has no library behind it, so its Music and Incoming are empty folders: a library its
//! catalogue never read would be read in at the start, and the step's files part would meet it. The file slskd serves is the layout's.
//!
//! The queue as this test serves it (MAP-SK17-PART4 §3, G10), nothing else of it changed: lists without recordings, each track the
//! served file's length, each offered file its size and length, so slskd's copies of one layout file pass the real gate and AcoustID
//! is never asked (OQ-GATE1 (a)). The 18 found items' "searched again, a miss" needs each record's counting list, which no start
//! reads offline: the miss is proven in the unit twins (`the_found_items_of_0163_are_searched_again_once_given_their_record`,
//! `the_waiting_and_stuck_items_of_0163_go_on_as_today`); here they go back to waiting and never start, their why naming MusicBrainz,
//! switched off (Part 4's gaps, 2026-11-20). The step's report is held to the one the same start makes of 0.16.2's data alone.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{Path as Part, Query, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

use super::{ONE_AT_A_TIME, Server, TIDE, data_0_16, differences, make_layout, settings, soulseek_takes_incoming, temp};

/// The queue fixture's folder: its queue, and its requests as rows (`requests.sql`) and as JSON (`requests.json`).
const HERE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/0.16.3");

/// slskd's API key as 0.16.3 kept it.
const KEY: &str = "k";

/// The connections a server can switch off that a start could otherwise ask (`off:<name>` among the server's choices): off.
const OFF: [&str; 3] = ["musicbrainz", "discogs", "bandcamp"];

/// A batch as the fake slskd holds it.
struct Batch {
    id: String,
    user: String,
    /// Its files as the user shares them, in order.
    files: Vec<String>,
    /// How far it was when slskd started (0.16.3's two), else 0.
    percent: f64,
    reads: u32,
    done: bool,
    /// Asked for here, not one slskd held from before.
    asked: bool,
}

/// What the fake slskd holds and was asked.
#[derive(Default)]
struct Book {
    in_flight: u32,
    batches: Vec<Batch>,
    /// Every operation answered, in turn.
    operations: Vec<String>,
    /// The searches it knows, by id; the texts searched, in turn.
    searches: BTreeSet<String>,
    searched: Vec<String>,
    /// The transfers dropped, by id.
    dropped: Vec<String>,
    violations: Vec<String>,
}

/// slskd as 0.16.3 left it (SK4.7; SCENARIOS-SK17's "Shapes and fakes"), on this machine:
/// - it holds 0.16.3's downloads (`knows`), each going till its second read;
/// - a batch asked for is taken and goes till its second read; done, each of its files is a copy of `served` in its folder
///   (`cratarr/<batch>`, the destination Cratarr names), one transfer a file;
/// - a search is taken, complete at once, and finds nothing;
/// - it takes one operation at a time (a batch or a search asked for, a transfer dropped, a search forgotten), each answered after
///   `slow` ms. One arriving while another is being answered gets 429, "Only one concurrent operation is permitted", and is a
///   violation: Cratarr caused it;
/// - violations also: a batch asked of a user whose batch still has a transfer not done (0.16.3's two among them), a batch id taken
///   twice, a batch not into its own folder, a transfer dropped that it does not know or not removed from its list, a search
///   forgotten that it does not know, a call without the key, and any call it has no answer for.
#[derive(Clone)]
struct Slskd0163 {
    url: String,
    book: Arc<Mutex<Book>>,
    completed: PathBuf,
    served: PathBuf,
    slow: u64,
}

impl Slskd0163 {
    async fn start(completed: &Path, served: &Path, slow: u64) -> Self {
        use axum::routing::{delete, get, post};
        std::fs::create_dir_all(completed).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let me = Self { url: format!("http://{}", listener.local_addr().unwrap()), book: Arc::default(), completed: completed.to_path_buf(), served: served.to_path_buf(), slow };
        let app = axum::Router::new()
            .route("/api/v0/application", get(application).fallback(unexpected))
            .route("/api/v0/application/version", get(version).fallback(unexpected))
            .route("/api/v0/searches", post(take_search).fallback(unexpected))
            .route("/api/v0/searches/{id}", get(search_state).delete(forget_search).fallback(unexpected))
            .route("/api/v0/searches/{id}/responses", get(responses).fallback(unexpected))
            .route("/api/v0/transfers/downloads", get(downloads).fallback(unexpected))
            .route("/api/v0/transfers/downloads/batches", post(take_batch).fallback(unexpected))
            .route("/api/v0/transfers/downloads/batches/{id}", get(read_batch).fallback(unexpected))
            .route("/api/v0/transfers/downloads/{user}/{id}", delete(drop_transfer).fallback(unexpected))
            .fallback(unexpected)
            .with_state(me.clone());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        me
    }

    fn book(&self) -> std::sync::MutexGuard<'_, Book> {
        self.book.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A batch slskd held when it started: `user`'s `files`, `percent` done.
    fn knows(&self, id: &str, user: &str, files: Vec<String>, percent: f64) {
        self.book().batches.push(Batch { id: id.into(), user: user.into(), files, percent, reads: 0, done: false, asked: false });
    }

    /// Every batch, in the order it held or took them: (id, user, files, asked here, done).
    fn batches(&self) -> Vec<(String, String, Vec<String>, bool, bool)> {
        self.book().batches.iter().map(|b| (b.id.clone(), b.user.clone(), b.files.clone(), b.asked, b.done)).collect()
    }

    fn violations(&self) -> Vec<String> {
        self.book().violations.clone()
    }

    fn violation(&self, what: String) {
        self.book().violations.push(what);
    }

    /// What it holds and was asked, in a few lines, for a failure to say.
    fn summary(&self) -> String {
        let b = self.book();
        let batches: Vec<String> = b.batches.iter().map(|x| format!("{} {} reads {} {}", x.id, x.user, x.reads, if x.done { "done" } else { "going" })).collect();
        format!("{} operations, {} searches, {} dropped; batches {batches:?}; violations {:?}", b.operations.len(), b.searched.len(), b.dropped.len(), b.violations)
    }
}

/// A batch's transfers as slskd lists them: one a file, going or done.
fn transfers(x: &Batch) -> Vec<Value> {
    let (state, percent) = if x.done { ("Completed, Succeeded", 100.0) } else { ("InProgress", x.percent) };
    x.files.iter().enumerate().map(|(n, f)| json!({ "id": format!("{}-{n}", x.id), "username": x.user, "direction": "Download", "filename": f, "state": state, "percentComplete": percent })).collect()
}

/// Whether the call carries slskd's key; one without is a violation, refused.
fn keyed(me: &Slskd0163, headers: &HeaderMap, uri: &Uri) -> Result<(), Response> {
    if headers.get("x-api-key").is_some_and(|k| k == KEY) {
        return Ok(());
    }
    me.violation(format!("a call without its API key: {uri}"));
    Err(StatusCode::UNAUTHORIZED.into_response())
}

/// An operation arriving: taken, and answered after `slow`; one arriving while another is being answered gets 429, a violation.
async fn arrive(me: &Slskd0163, what: &str) -> Result<(), Response> {
    {
        let mut b = me.book();
        if b.in_flight > 0 {
            b.violations.push(format!("an operation while another was answered: {what}"));
            return Err((StatusCode::TOO_MANY_REQUESTS, ONE_AT_A_TIME).into_response());
        }
        b.in_flight += 1;
    }
    tokio::time::sleep(Duration::from_millis(me.slow)).await;
    let mut b = me.book();
    b.in_flight -= 1;
    b.operations.push(what.to_string());
    Ok(())
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

async fn take_batch(State(me): State<Slskd0163>, headers: HeaderMap, uri: Uri, axum::Json(body): axum::Json<Value>) -> Response {
    if let Err(r) = keyed(&me, &headers, &uri) {
        return r;
    }
    let (id, user) = (text(&body["id"]), text(&body["username"]));
    let files: Vec<String> = body["files"].as_array().into_iter().flatten().map(|f| text(&f["filename"])).collect();
    if let Err(r) = arrive(&me, &format!("a batch of {user}")).await {
        return r;
    }
    let mut b = me.book();
    if id.is_empty() || files.is_empty() || b.batches.iter().any(|x| x.id == id) {
        b.violations.push(format!("a batch of {user} with no id or no files, or its id taken before: {id}"));
        return (StatusCode::CONFLICT, "not expected").into_response();
    }
    if let Some(x) = b.batches.iter().find(|x| x.user == user && !x.done) {
        let v = format!("a batch asked of {user} while their batch {} still has a transfer not done", x.id);
        b.violations.push(v);
    }
    if body["options"]["destination"].as_str() != Some(format!("cratarr/{id}").as_str()) {
        b.violations.push(format!("batch {id} not into its own folder: {}", body["options"]));
    }
    b.batches.push(Batch { id: id.clone(), user: user.clone(), files, percent: 0.0, reads: 0, done: false, asked: true });
    let listed = transfers(b.batches.last().unwrap());
    axum::Json(json!({ "batch": { "id": id, "username": user, "transfers": listed }, "failures": [] })).into_response()
}

async fn read_batch(State(me): State<Slskd0163>, headers: HeaderMap, uri: Uri, Part(id): Part<String>) -> Response {
    if let Err(r) = keyed(&me, &headers, &uri) {
        return r;
    }
    let mut b = me.book();
    let Some(x) = b.batches.iter_mut().find(|x| x.id == id) else { return StatusCode::NOT_FOUND.into_response() };
    x.reads += 1;
    if !x.done && x.reads >= 2 {
        let dir = me.completed.join("cratarr").join(&id);
        std::fs::create_dir_all(&dir).unwrap();
        for f in &x.files {
            std::fs::copy(&me.served, dir.join(f.rsplit(['\\', '/']).next().unwrap_or(f))).unwrap();
        }
        x.done = true;
    }
    axum::Json(json!({ "id": id, "username": x.user, "transfers": transfers(x) })).into_response()
}

async fn drop_transfer(State(me): State<Slskd0163>, headers: HeaderMap, uri: Uri, Part((user, id)): Part<(String, String)>, Query(q): Query<BTreeMap<String, String>>) -> Response {
    if let Err(r) = keyed(&me, &headers, &uri) {
        return r;
    }
    if let Err(r) = arrive(&me, &format!("a transfer of {user} dropped")).await {
        return r;
    }
    let mut b = me.book();
    if !b.batches.iter().any(|x| x.user == user && (0..x.files.len()).any(|n| format!("{}-{n}", x.id) == id)) {
        b.violations.push(format!("a transfer dropped that it does not know: {user} {id}"));
        return StatusCode::NOT_FOUND.into_response();
    }
    if q.get("remove").map(String::as_str) != Some("true") {
        b.violations.push(format!("a transfer cancelled but not removed from the list: {user} {id}"));
    }
    b.dropped.push(id);
    StatusCode::NO_CONTENT.into_response()
}

async fn take_search(State(me): State<Slskd0163>, headers: HeaderMap, uri: Uri, axum::Json(body): axum::Json<Value>) -> Response {
    if let Err(r) = keyed(&me, &headers, &uri) {
        return r;
    }
    let (id, searched) = (text(&body["id"]), text(&body["searchText"]));
    if let Err(r) = arrive(&me, &format!("a search for {searched}")).await {
        return r;
    }
    let mut b = me.book();
    if id.is_empty() || !b.searches.insert(id.clone()) {
        b.violations.push(format!("a search with no id, or its id taken before: {id}"));
        return (StatusCode::CONFLICT, "not expected").into_response();
    }
    b.searched.push(searched.clone());
    axum::Json(json!({ "id": id, "searchText": searched, "isComplete": false, "state": "InProgress", "responseCount": 0 })).into_response()
}

async fn search_state(State(me): State<Slskd0163>, headers: HeaderMap, uri: Uri, Part(id): Part<String>) -> Response {
    if let Err(r) = keyed(&me, &headers, &uri) {
        return r;
    }
    if !me.book().searches.contains(&id) {
        return StatusCode::NOT_FOUND.into_response();
    }
    axum::Json(json!({ "id": id, "isComplete": true, "state": "Completed, TimedOut", "responseCount": 0, "fileCount": 0, "lockedFileCount": 0 })).into_response()
}

async fn responses(State(me): State<Slskd0163>, headers: HeaderMap, uri: Uri, Part(id): Part<String>) -> Response {
    if let Err(r) = keyed(&me, &headers, &uri) {
        return r;
    }
    if !me.book().searches.contains(&id) {
        return StatusCode::NOT_FOUND.into_response();
    }
    axum::Json(json!([])).into_response()
}

async fn forget_search(State(me): State<Slskd0163>, headers: HeaderMap, uri: Uri, Part(id): Part<String>) -> Response {
    if let Err(r) = keyed(&me, &headers, &uri) {
        return r;
    }
    if let Err(r) = arrive(&me, "a search forgotten").await {
        return r;
    }
    let mut b = me.book();
    if !b.searches.remove(&id) {
        b.violations.push(format!("a search forgotten that it does not know: {id}"));
        return StatusCode::NOT_FOUND.into_response();
    }
    StatusCode::NO_CONTENT.into_response()
}

async fn application(State(me): State<Slskd0163>, headers: HeaderMap, uri: Uri) -> Response {
    if let Err(r) = keyed(&me, &headers, &uri) {
        return r;
    }
    axum::Json(json!({ "server": { "isLoggedIn": true, "isConnected": true, "state": "Connected, LoggedIn" } })).into_response()
}

async fn version(State(me): State<Slskd0163>, headers: HeaderMap, uri: Uri) -> Response {
    if let Err(r) = keyed(&me, &headers, &uri) {
        return r;
    }
    axum::Json(json!("0.26.0")).into_response()
}

async fn downloads(State(me): State<Slskd0163>, headers: HeaderMap, uri: Uri) -> Response {
    if let Err(r) = keyed(&me, &headers, &uri) {
        return r;
    }
    axum::Json(json!([])).into_response()
}

async fn unexpected(State(me): State<Slskd0163>, method: axum::http::Method, uri: Uri) -> Response {
    me.violation(format!("an unexpected call: {method} {uri}"));
    (StatusCode::IM_A_TEAPOT, "not expected").into_response()
}

/// FX01's data folder at `data` (`tests/fixtures/0.16`, as the suite's `data_0_16` makes it), the connections in `OFF` switched off,
/// and settings as 0.16 writes them: Music and Incoming at `library`, empty, Incoming taking Soulseek's records.
fn fx01(data: &Path, library: &Path) {
    for dir in ["music", "incoming", "own"] {
        std::fs::create_dir_all(library.join(dir)).unwrap();
    }
    data_0_16(data);
    let db = rusqlite::Connection::open(data.join("userdata.sqlite")).unwrap();
    for name in OFF {
        db.execute("INSERT OR REPLACE INTO import_choices (source, choice) VALUES (?1, '1')", [format!("off:{name}")]).unwrap();
    }
    drop(db);
    let (music, incoming) = (library.join("music"), library.join("incoming"));
    settings(data, &[(&music, "main"), (&incoming, "incoming")], &library.join("own"));
    soulseek_takes_incoming(data, &incoming);
}

/// The data folder as 0.16.3 leaves it (SK4.7's Given): FX01's (`fx01`), the queue fixture's requests in its user data, slskd at `url`
/// as 0.16.3 kept it (its address, key and completed-downloads folder among the server's choices), and `queue` as `soulseek.json`.
fn data_0_16_3(data: &Path, library: &Path, url: &str, completed: &Path, queue: &[u8]) {
    fx01(data, library);
    let db = rusqlite::Connection::open(data.join("userdata.sqlite")).unwrap();
    let requests = std::fs::read_to_string(Path::new(HERE).join("requests.sql")).unwrap();
    db.execute_batch(&format!("BEGIN;\n{requests}\nCOMMIT;")).unwrap();
    let folder = completed.to_string_lossy();
    for (source, choice) in [("slskd:url", url), ("slskd:key", KEY), ("slskd:folder", &*folder)] {
        db.execute("INSERT OR REPLACE INTO import_choices (source, choice) VALUES (?1, ?2)", [source, choice]).unwrap();
    }
    drop(db);
    std::fs::write(data.join("soulseek.json"), queue).unwrap();
}

/// 0.16.3's queue fixture as this test serves it (MAP-SK17-PART4 §3, G10): each track's recording taken out (a list without
/// recordings: AcoustID is not asked) and its length the served file's; each offered file the served file's size and length.
fn queue_served(served: &Path) -> Value {
    use lofty::prelude::*;
    let secs = lofty::read_from_path(served).unwrap().properties().duration().as_secs_f64();
    let size = std::fs::metadata(served).unwrap().len();
    let mut q: Value = serde_json::from_slice(&std::fs::read(Path::new(HERE).join("soulseek.json")).unwrap()).unwrap();
    for i in q["items"].as_array_mut().unwrap() {
        for t in i["tracks"].as_array_mut().into_iter().flatten() {
            let t = t.as_object_mut().unwrap();
            t.remove("recording");
            t.insert("length_ms".into(), json!((secs * 1000.0).round() as i64));
        }
        for f in i["folders"].as_array_mut().into_iter().flatten() {
            for o in f["files"].as_array_mut().into_iter().flatten() {
                o["size"] = json!(size);
                o["length"] = json!(secs.round() as i64);
            }
        }
    }
    q
}

/// The step's report as `/api/admin/migration` lists it once its part is there: every part, without when each was written.
async fn report_of(s: &Server) -> Value {
    s.until("the step's report", Duration::from_secs(60), || async { s.get("/api/admin/migration").await["parts"].get("references").is_some() }).await;
    let mut parts = s.get("/api/admin/migration").await["parts"].clone();
    for part in parts.as_object_mut().unwrap().values_mut() {
        if let Some(p) = part.as_object_mut() {
            p.remove("at");
        }
    }
    parts
}

/// The step's report on 0.16.2's data alone (FX01), as the same start makes it, in a folder of its own under `root`: what SK4.7's
/// report is held to.
async fn report_of_0162(root: &Path) -> Value {
    let (library, data) = (root.join("library"), root.join("data"));
    fx01(&data, &library);
    let s = Server::start(&data).await;
    let report = report_of(&s).await;
    s.stop();
    report
}

/// The step's report on 0.16.2's data with 0.16.3's requests beside its own: each request row read and carried, each given a
/// record of its own (its release group known to nothing) holding that release group as its reference, listed so.
fn with_requests(mut report: Value, requests: &[Value]) -> Value {
    let n = requests.len() as u64;
    let r = &mut report["references"];
    for t in r["tables"].as_array_mut().unwrap() {
        let more: &[&str] = match t["name"].as_str() {
            Some("request") => &["before", "after", "moved"],
            Some("references") => &["after", "moved"],
            _ => &[],
        };
        for k in more {
            t[*k] = json!(t[*k].as_u64().unwrap_or(0) + n);
        }
    }
    r["records_made_for_requests"] = json!(r["records_made_for_requests"].as_u64().unwrap_or(0) + n);
    let made = requests.iter().map(|q| json!(format!("{} {} - {}", q["id"], text(&q["artist"]), text(&q["album"]))));
    r["records_made_for"].as_array_mut().unwrap().extend(made);
    report
}

/// Each item in a line: its id, state and why.
fn summary(items: &BTreeMap<String, Value>) -> String {
    items.iter().map(|(id, i)| format!("\n  {id} {} {}", text(&i["state"]), text(&i["why"]).chars().take(100).collect::<String>())).collect()
}

/// A why with a 429's words in it.
fn says_429(why: &str) -> bool {
    why.contains("429") || why.contains("Too Many Requests")
}

/// The items of a list, by id.
fn by_id(items: &Value) -> BTreeMap<String, Value> {
    items.as_array().into_iter().flatten().map(|i| (text(&i["id"]), i.clone())).collect()
}

/// SK4.7: 0.16.3's queue through 0.17's first start. The step's report as for 0.16.2; 0.16.3's two downloads (b-01, b-02) followed
/// and placed; the 20 records it held for slskd's 429 fetched one operation at a time, never two of one user, each placed, no why
/// ever saying 429; the 18 found back to waiting, never started; nobody failed; `soulseek.json.016` kept aside once, as the queue
/// was. Never a violation of slskd's, an item lost, a 429 in any why.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the end-to-end suite: the Dockerfile's e2e stage"]
async fn the_0163_queue_upgrades_end_to_end() {
    let root = temp("sk47");
    let (library, data, completed) = (root.join("library"), root.join("data"), root.join("slskd"));
    let incoming = library.join("incoming");
    // The file slskd serves: one of the layout's, kept outside the libraries.
    make_layout(&root.join("layout"));
    let served = root.join("served").join("01 Slack Water.flac");
    std::fs::create_dir_all(served.parent().unwrap()).unwrap();
    std::fs::copy(root.join("layout").join("music").join(TIDE).join("01 Slack Water.flac"), &served).unwrap();
    let as_for_0162 = report_of_0162(&root.join("0.16.2")).await;

    // The Given.
    let queue = queue_served(&served);
    let bytes = serde_json::to_vec(&queue).unwrap();
    let was = by_id(&queue["items"]);
    let in_state = |state: &str| -> Vec<String> { was.iter().filter(|(_, i)| i["state"] == state).map(|(id, _)| id.clone()).collect() };
    let (fetching, approved, found) = (in_state("fetching"), in_state("approved"), in_state("found"));
    assert_eq!((was.len(), fetching.len(), approved.len(), found.len()), (57, 2, 20, 18), "the fixture");
    let fetched: Vec<String> = fetching.iter().chain(&approved).cloned().collect();
    let chosen = |id: &str| -> Value { was[id]["folders"][was[id]["chosen"].as_u64().unwrap() as usize].clone() };
    let files_of = |id: &str| -> Vec<String> { chosen(id)["files"].as_array().unwrap().iter().map(|f| text(&f["filename"])).collect() };
    let slskd = Slskd0163::start(&completed, &served, 300).await;
    for id in &fetching {
        slskd.knows(was[id]["batch"].as_str().unwrap(), chosen(id)["user"].as_str().unwrap(), files_of(id), was[id]["percent"].as_f64().unwrap());
    }
    data_0_16_3(&data, &library, &slskd.url, &completed, &bytes);
    let requests: Vec<Value> = serde_json::from_slice(&std::fs::read(Path::new(HERE).join("requests.json")).unwrap()).unwrap();

    // 0.17 starts, once.
    let s = Server::start(&data).await;
    let report = report_of(&s).await;
    let (mut whys, mut never): (BTreeSet<String>, BTreeSet<String>) = Default::default();
    let started = Instant::now();
    let items = loop {
        let now = by_id(&s.get("/api/soulseek").await["items"]);
        whys.extend(now.values().filter_map(|i| i["why"].as_str().map(str::to_string)));
        let said_429: Vec<&String> = whys.iter().filter(|w| says_429(w)).collect();
        assert!(said_429.is_empty(), "a 429 in a why, {} s after the start: {said_429:?}{}", started.elapsed().as_secs(), summary(&now));
        for (id, w) in &was {
            let Some(i) = now.get(id) else {
                never.insert(format!("{id} lost"));
                continue;
            };
            let blamed: Vec<&Value> = i["failed"].as_array().into_iter().flatten().filter(|u| !w["failed"].as_array().is_some_and(|f| f.contains(u))).collect();
            if !blamed.is_empty() {
                never.insert(format!("{id}: {blamed:?} failed"));
            }
        }
        for id in &found {
            let state = now.get(id).map(|i| text(&i["state"])).unwrap_or_default();
            if ["approved", "fetching", "checking", "placed"].contains(&state.as_str()) {
                never.insert(format!("{id} found by 0.16 started: {state}"));
            }
        }
        let placed = fetched.iter().all(|id| now.get(id).is_some_and(|i| i["state"] == "placed"));
        let searched = found.iter().all(|id| now.get(id).is_some_and(|i| i["state"] == "waiting" && text(&i["why"]).contains("MusicBrainz")));
        if placed && searched {
            break now;
        }
        assert!(started.elapsed() < Duration::from_secs(600), "the 22 placed and the 18 searched again:{}\nslskd: {}\nnever: {never:?}", summary(&now), slskd.summary());
        tokio::time::sleep(Duration::from_secs(1)).await;
    };

    // Never: an item lost, a user blamed, one 0.16 found started, a 429 in any why, as the list kept it at every look and on disk.
    assert!(never.is_empty(), "{never:#?}");
    let on_disk = by_id(&serde_json::from_slice::<Value>(&std::fs::read(data.join("soulseek.json")).unwrap()).unwrap()["items"]);
    for id in was.keys() {
        assert!(on_disk.contains_key(id), "{id} kept in soulseek.json");
    }
    assert!(!on_disk.values().any(|i| says_429(&text(&i["why"]))), "no 429 in soulseek.json");

    // b-01 and b-02 followed, never asked for again, and placed, each in its own folder with its files.
    for (id, batch, folder) in [("ss-1", "b-01", "Kite Theory/Slack Water 1 (2001)"), ("ss-2", "b-02", "Noor Field/Neap 2 (2002)")] {
        assert_eq!(was[id]["batch"], batch, "the fixture");
        let to = incoming.join(folder);
        assert_eq!((text(&items[id]["state"]), text(&items[id]["placed"])), ("placed".into(), to.display().to_string()), "{id}: {}", items[id]);
        assert_eq!(text(&on_disk[id]["state"]), "placed", "{id} in soulseek.json");
        for f in files_of(id) {
            let name = f.rsplit('\\').next().unwrap().to_string();
            assert!(to.join(&name).is_file(), "{id}: {name} placed");
        }
    }

    // The 20 fetched one operation at a time, never two of one user (slskd's own violations), each one batch, as 0.16.3 picked it.
    let batches = slskd.batches();
    let asked: Vec<&(String, String, Vec<String>, bool, bool)> = batches.iter().filter(|b| b.3).collect();
    assert_eq!(asked.len(), 20, "20 batches asked for: {}", slskd.summary());
    for id in &fetched {
        let of_it: Vec<&str> = batches.iter().filter(|b| b.2 == files_of(id)).map(|b| b.0.as_str()).collect();
        assert_eq!(of_it.len(), 1, "{id}: one batch, never two: {of_it:?}");
        if fetching.contains(id) {
            assert_eq!(of_it, [text(&was[id]["batch"])], "{id}: 0.16.3's own batch followed, never asked for again");
        }
        let i = &items[id];
        assert_eq!((i["auto"].as_bool(), i["chosen"].as_u64()), (Some(false), was[id]["chosen"].as_u64()), "{id}: its pick as 0.16.3 kept it, not Cratarr's: {i}");
    }
    assert!(slskd.violations().is_empty(), "{:#?}", slskd.violations());

    // The 18 found by 0.16: back to waiting, their folders gone, never started; their list's source switched off, so nothing searched.
    for id in &found {
        let i = &items[id];
        assert_eq!((text(&i["state"]), &i["folders"]), ("waiting".into(), &json!([])), "{id}: {i}");
        assert!(text(&i["why"]).contains("MusicBrainz is switched off"), "{id}: {i}");
    }

    // 0.16.3's queue kept aside once, as it was.
    assert_eq!(std::fs::read(data.join("soulseek.json.016")).unwrap(), bytes, "soulseek.json.016 as 0.16.3 left the queue");

    // The step's report as for 0.16.2, its requests with 0.16.3's.
    let mut differ = Vec::new();
    differences("", &with_requests(as_for_0162, &requests), &report, &mut differ);
    assert!(differ.is_empty(), "the step's report as for 0.16.2: {differ:#?}");
    s.stop();
    std::fs::remove_dir_all(&root).ok();
}
