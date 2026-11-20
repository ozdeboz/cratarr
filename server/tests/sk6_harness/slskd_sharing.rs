//! slskd sharing folders (SCENARIOS-SK17-GATE "Shapes", `slskd_sharing`): a fake slskd on this machine for the end-to-end sourcing
//! scenarios, built as the suite's other slskd fakes are (an `axum::Router` on a free port) and keeping SCENARIOS-SK17's rules for
//! fakes: it records every operation, read and drop, and a call it does not expect is a violation, answered with an error.
//!
//! - **Shares:** each a user and a folder on this machine ([`Share`]), offered as `Music\<folder>\<file>` with each file's real
//!   size and length (whole seconds, as slskd gives them), a free upload slot, no queue, 16 bits at 44.1 kHz and the share's upload
//!   speed. Each can be switched off and on ([`Share::switch`]).
//! - **A search** is taken, complete at once, and answered with every share switched on when it was taken, one answer a user,
//!   whatever its text: Cratarr judges which hold its record. Forgotten when Cratarr asks; read after, it is not known (404).
//! - **A batch** names a user and files: those files of that user's shares (switched on or not) are copied into
//!   `<completed>/<destination>/` (Cratarr's `cratarr/<batch>`) before it is answered, so they lie there whole as slskd leaves a
//!   finished download. Read back, it is one transfer a file, "Completed, Succeeded", 100 %. A batch it never took reads 404.
//! - **`busy_first(n)`:** the first n batch POSTs (from then, and again from each `mark`) are answered 429, "Only one concurrent
//!   operation is permitted. Wait until the previous operation completes", and nothing is taken: slskd busy with its own work, no
//!   violation.
//! - **Recorded:** every operation (a batch or a search started, a transfer dropped, a search forgotten) with its answer, and every
//!   read. A transfer dropped with `remove=true` leaves slskd's downloads list.
//! - **Idle reads:** its version, whether it is signed in to Soulseek, and its downloads list (the transfers not dropped).
//! - **Violations:** a batch naming a file its user does not share; a batch id taken twice; an operation arriving while another is
//!   still being answered (SK2's lane); a drop of a transfer or a forget of a search it does not know, or a drop without
//!   `remove=true`; a call without an API key; any call it has no answer for.
//! - **`mark()`** (at a restart): `operations`, `posts`, `reads`, `searches`, `dropped` and `every_search_forgotten` count from
//!   then on. Violations are kept whole.
//!
//! Self-contained: the server's own tests compile this file too (`fixtures::gate`), to read its answers with Cratarr's real slskd
//! client. It names nothing of `e2e.rs`.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{Path as Part, Query, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use serde_json::{Value, json};

/// slskd's words when it is busy with another operation.
pub const ONE_AT_A_TIME: &str = "Only one concurrent operation is permitted. Wait until the previous operation completes";

/// One user's folder shared: its files offered as `Music\<folder>\<file>`.
#[derive(Debug, Clone)]
pub struct Share {
    pub user: String,
    /// The folder on this machine whose files are shared.
    pub dir: PathBuf,
    /// Its name as shared.
    pub folder: String,
    /// Its upload speed, as an answer says it (bytes a second): the faster ranks first, all else equal.
    pub upload_speed: i64,
    on: Arc<AtomicBool>,
}

impl Share {
    /// `user` sharing the files of `dir` as `Music\<folder>`, switched on.
    pub fn new(user: &str, dir: &Path, folder: &str) -> Share {
        Share { user: user.to_string(), dir: dir.to_path_buf(), folder: folder.to_string(), upload_speed: 1_000_000, on: Arc::new(AtomicBool::new(true)) }
    }

    /// Its upload speed.
    pub fn speed(mut self, speed: i64) -> Share {
        self.upload_speed = speed;
        self
    }

    /// Offered to searches from now (true) or not (false). The fake reads the same switch.
    pub fn switch(&self, on: bool) {
        self.on.store(on, Ordering::SeqCst);
    }

    pub fn is_on(&self) -> bool {
        self.on.load(Ordering::SeqCst)
    }

    /// Its files, by name: (name, where it lies). Hidden files are not shared.
    pub fn files(&self) -> Vec<(String, PathBuf)> {
        let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_file())
            .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
            .filter(|(name, _)| !name.starts_with('.'))
            .collect();
        out.sort();
        out
    }

    /// A file's name as shared: `Music\<folder>\<name>`.
    pub fn shared_as(&self, name: &str) -> String {
        format!("Music\\{}\\{name}", self.folder)
    }

    /// Each file's name as shared, in order: what a batch of the whole folder names.
    pub fn offered(&self) -> Vec<String> {
        self.files().iter().map(|(name, _)| self.shared_as(name)).collect()
    }

    /// The file on this machine that `filename` (as shared) names, where this share has it.
    fn lies(&self, filename: &str) -> Option<PathBuf> {
        self.files().into_iter().find(|(name, _)| self.shared_as(name) == filename).map(|(_, p)| p)
    }

    /// Its files as a search's answer lists them.
    fn listing(&self) -> Vec<Value> {
        self.files()
            .iter()
            .map(|(name, p)| {
                let ext = name.rsplit_once('.').map(|(_, x)| x.to_lowercase()).unwrap_or_default();
                json!({
                    "code": 1, "filename": self.shared_as(name), "size": std::fs::metadata(p).map(|m| m.len()).unwrap_or(0),
                    "extension": ext, "bitDepth": 16, "sampleRate": 44_100, "length": length_secs(p), "isLocked": false,
                })
            })
            .collect()
    }
}

/// A file's length in whole seconds, as slskd gives it; none where it is not audio that can be read.
fn length_secs(p: &Path) -> Option<i64> {
    use lofty::prelude::*;
    let f = lofty::read_from_path(p).ok()?;
    Some(f.properties().duration().as_secs_f64().round() as i64)
}

/// What an operation asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Batch,
    Search,
    DropTransfer,
    ForgetSearch,
}

/// One operation as it reached slskd, and its answer.
#[derive(Debug, Clone)]
pub struct Op {
    pub kind: Kind,
    /// A batch's user; a dropped transfer's; empty for a search.
    pub user: String,
    /// A batch's id; a dropped transfer's batch; a search's id.
    pub batch: String,
    /// A batch's files, as it named them.
    pub files: Vec<String>,
    /// A dropped transfer's id.
    pub transfer: String,
    /// A search's text.
    pub text: String,
    /// Its answer's status.
    pub status: u16,
    /// When it arrived, in ms since the fake started.
    pub at_ms: u64,
}

/// A batch taken: whose, and its transfers: (id, the file as shared, its size).
#[derive(Debug, Clone)]
struct Taken {
    user: String,
    transfers: Vec<(String, String, u64)>,
    removed: BTreeSet<String>,
}

#[derive(Debug, Clone)]
struct Searched {
    text: String,
    answers: Vec<Value>,
    forgotten: bool,
}

#[derive(Default)]
struct Book {
    shares: Vec<Share>,
    completed: PathBuf,
    busy_first: u32,
    busy_left: u32,
    slow_ms: u64,
    in_flight: u32,
    ops: Vec<Op>,
    reads: Vec<String>,
    taken: BTreeMap<String, Taken>,
    searches: BTreeMap<String, Searched>,
    violations: Vec<String>,
    /// Where `ops` and `reads` stood at the last `mark`.
    mark: (usize, usize),
}

/// The fake: its address, and what it was asked.
#[derive(Clone)]
pub struct Sharing {
    pub url: String,
    book: Arc<Mutex<Book>>,
    started: Instant,
}

/// slskd on this machine, sharing `shares`, its completed downloads in `completed`. In a runtime.
pub async fn slskd_sharing(completed: &Path, shares: &[&Share]) -> Sharing {
    let book = Book { shares: shares.iter().map(|s| (*s).clone()).collect(), completed: completed.to_path_buf(), ..Book::default() };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let me = Sharing { url: format!("http://{}", listener.local_addr().unwrap()), book: Arc::new(Mutex::new(book)), started: Instant::now() };
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

impl Sharing {
    fn book(&self) -> std::sync::MutexGuard<'_, Book> {
        self.book.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The first `n` batch POSTs from now, and again from each `mark`, answered 429: slskd busy with its own work (no violation).
    pub fn busy_first(&self, n: u32) {
        let mut b = self.book();
        (b.busy_first, b.busy_left) = (n, n);
    }

    /// Each operation answered after `ms`, so one arriving meanwhile would meet another being answered.
    pub fn slow(&self, ms: u64) {
        self.book().slow_ms = ms;
    }

    /// From now on (a restart): what it is asked is counted from here, and `busy_first` holds again.
    pub fn mark(&self) {
        let mut b = self.book();
        b.mark = (b.ops.len(), b.reads.len());
        b.busy_left = b.busy_first;
    }

    /// Every operation since the mark, in the order they were answered.
    pub fn operations(&self) -> Vec<Op> {
        let b = self.book();
        b.ops[b.mark.0..].to_vec()
    }

    /// Every batch POST since the mark, each try (a 429 too): its user, batch, files and answer.
    pub fn posts(&self) -> Vec<Op> {
        self.operations().into_iter().filter(|o| o.kind == Kind::Batch).collect()
    }

    /// The users of `posts`, in turn.
    pub fn post_users(&self) -> Vec<String> {
        self.posts().into_iter().map(|o| o.user).collect()
    }

    /// The users of the `posts` whose files lie in a folder shared as `folder` ("Glass Harbour - Tide Tables (2012)"), in turn.
    pub fn post_users_of(&self, folder: &str) -> Vec<String> {
        let of = |f: &String| f.rsplit_once('\\').is_some_and(|(dir, _)| dir.rsplit('\\').next() == Some(folder));
        self.posts().into_iter().filter(|o| !o.files.is_empty() && o.files.iter().all(of)).map(|o| o.user).collect()
    }

    /// The transfers slskd gave a batch it took, by id, in its files' order (empty for one it never took).
    pub fn transfers_of(&self, batch: &str) -> Vec<String> {
        self.book().taken.get(batch).map(|t| t.transfers.iter().map(|(id, _, _)| id.clone()).collect()).unwrap_or_default()
    }

    /// Every transfer dropped since the mark (DELETE … `remove=true`), by id, in turn.
    pub fn dropped(&self) -> Vec<String> {
        self.operations().into_iter().filter(|o| o.kind == Kind::DropTransfer && o.status < 300).map(|o| o.transfer).collect()
    }

    /// The transfers of `batch` dropped since the mark, in turn.
    pub fn dropped_of(&self, batch: &str) -> Vec<String> {
        self.operations().into_iter().filter(|o| o.kind == Kind::DropTransfer && o.status < 300 && o.batch == batch).map(|o| o.transfer).collect()
    }

    /// The texts of the searches taken since the mark, in turn.
    pub fn searches(&self) -> Vec<String> {
        self.operations().into_iter().filter(|o| o.kind == Kind::Search && o.status < 300).map(|o| o.text).collect()
    }

    /// Whether every search taken since the mark has been forgotten.
    pub fn every_search_forgotten(&self) -> bool {
        let ops = self.operations();
        let forgotten: BTreeSet<&str> = ops.iter().filter(|o| o.kind == Kind::ForgetSearch && o.status < 300).map(|o| o.batch.as_str()).collect();
        ops.iter().filter(|o| o.kind == Kind::Search && o.status < 300).all(|o| forgotten.contains(o.batch.as_str()))
    }

    /// Every read since the mark: its path.
    pub fn reads(&self) -> Vec<String> {
        let b = self.book();
        b.reads[b.mark.1..].to_vec()
    }

    /// Every violation since it started.
    pub fn violations(&self) -> Vec<String> {
        self.book().violations.clone()
    }

    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    fn violation(&self, what: String) {
        self.book().violations.push(what);
    }
}

/// An operation arriving (`busy`: one that `busy_first` may meet): taken now (counted in flight while it is answered, after the
/// fake's `slow`), else its 429: slskd's own work, or a violation where another operation is still being answered.
async fn arrive(me: &Sharing, mut op: Op, busy: bool) -> Result<Op, Response> {
    op.at_ms = me.now_ms();
    let slow = {
        let mut b = me.book();
        let own_work = busy && b.busy_left > 0;
        if own_work || b.in_flight > 0 {
            if own_work {
                b.busy_left -= 1;
            } else {
                b.violations.push(format!("an operation while another was answered: {:?} {} {}", op.kind, op.user, op.batch));
            }
            op.status = 429;
            b.ops.push(op);
            return Err((StatusCode::TOO_MANY_REQUESTS, ONE_AT_A_TIME).into_response());
        }
        b.in_flight += 1;
        b.slow_ms
    };
    if slow > 0 {
        tokio::time::sleep(Duration::from_millis(slow)).await;
    }
    Ok(op)
}

/// An operation answered: out of flight, recorded with its status.
fn answered(me: &Sharing, mut op: Op, status: StatusCode) {
    let mut b = me.book();
    b.in_flight = b.in_flight.saturating_sub(1);
    op.status = status.as_u16();
    b.ops.push(op);
}

fn blank(kind: Kind) -> Op {
    Op { kind, user: String::new(), batch: String::new(), files: Vec::new(), transfer: String::new(), text: String::new(), status: 0, at_ms: 0 }
}

/// Whether the call carries an API key, as slskd asks; one without is a violation, refused.
fn keyed(me: &Sharing, headers: &HeaderMap, uri: &Uri) -> Result<(), Response> {
    if headers.get("x-api-key").is_some_and(|k| !k.is_empty()) {
        return Ok(());
    }
    me.violation(format!("a call without an API key: {uri}"));
    Err(StatusCode::UNAUTHORIZED.into_response())
}

/// A read, recorded.
fn read(me: &Sharing, uri: &Uri) {
    me.book().reads.push(uri.path().to_string());
}

async fn application(State(me): State<Sharing>, headers: HeaderMap, uri: Uri) -> Response {
    if let Err(r) = keyed(&me, &headers, &uri) {
        return r;
    }
    read(&me, &uri);
    axum::Json(json!({ "server": { "isLoggedIn": true, "isConnected": true, "state": "Connected, LoggedIn" } })).into_response()
}

async fn version(State(me): State<Sharing>, headers: HeaderMap, uri: Uri) -> Response {
    if let Err(r) = keyed(&me, &headers, &uri) {
        return r;
    }
    read(&me, &uri);
    axum::Json(json!("0.26.0")).into_response()
}

async fn take_search(State(me): State<Sharing>, headers: HeaderMap, uri: Uri, axum::Json(body): axum::Json<Value>) -> Response {
    if let Err(r) = keyed(&me, &headers, &uri) {
        return r;
    }
    let id = body["id"].as_str().unwrap_or_default().to_string();
    let text = body["searchText"].as_str().unwrap_or_default().to_string();
    let op = Op { batch: id.clone(), text: text.clone(), ..blank(Kind::Search) };
    let op = match arrive(&me, op, false).await {
        Ok(op) => op,
        Err(r) => return r,
    };
    let known = me.book().searches.contains_key(&id);
    if id.is_empty() || known {
        me.violation(format!("a search with no id, or one taken twice: {id:?}"));
        answered(&me, op, StatusCode::CONFLICT);
        return (StatusCode::CONFLICT, "not expected").into_response();
    }
    // Every share switched on, one answer a user.
    let shares: Vec<Share> = me.book().shares.iter().filter(|s| s.is_on()).cloned().collect();
    let mut by_user: BTreeMap<String, (Vec<Value>, i64)> = BTreeMap::new();
    for s in &shares {
        let e = by_user.entry(s.user.clone()).or_default();
        e.0.extend(s.listing());
        e.1 = e.1.max(s.upload_speed);
    }
    let answers: Vec<Value> = by_user
        .into_iter()
        .enumerate()
        .map(|(n, (user, (files, speed)))| {
            json!({ "username": user, "token": n + 1, "fileCount": files.len(), "files": files, "lockedFileCount": 0, "lockedFiles": [], "hasFreeUploadSlot": true, "queueLength": 0, "uploadSpeed": speed })
        })
        .collect();
    let count = answers.len();
    me.book().searches.insert(id.clone(), Searched { text: text.clone(), answers, forgotten: false });
    answered(&me, op, StatusCode::OK);
    axum::Json(json!({ "id": id, "searchText": text, "isComplete": false, "state": "InProgress", "responseCount": count })).into_response()
}

async fn search_state(State(me): State<Sharing>, headers: HeaderMap, uri: Uri, Part(id): Part<String>) -> Response {
    if let Err(r) = keyed(&me, &headers, &uri) {
        return r;
    }
    read(&me, &uri);
    let b = me.book();
    match b.searches.get(&id).filter(|s| !s.forgotten) {
        Some(s) => {
            let files: usize = s.answers.iter().map(|a| a["fileCount"].as_u64().unwrap_or(0) as usize).sum();
            axum::Json(json!({ "id": id, "searchText": s.text, "isComplete": true, "state": "Completed, TimedOut", "responseCount": s.answers.len(), "fileCount": files, "lockedFileCount": 0 }))
                .into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn responses(State(me): State<Sharing>, headers: HeaderMap, uri: Uri, Part(id): Part<String>) -> Response {
    if let Err(r) = keyed(&me, &headers, &uri) {
        return r;
    }
    read(&me, &uri);
    let b = me.book();
    match b.searches.get(&id).filter(|s| !s.forgotten) {
        Some(s) => axum::Json(Value::Array(s.answers.clone())).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn forget_search(State(me): State<Sharing>, headers: HeaderMap, uri: Uri, Part(id): Part<String>) -> Response {
    if let Err(r) = keyed(&me, &headers, &uri) {
        return r;
    }
    let op = match arrive(&me, Op { batch: id.clone(), ..blank(Kind::ForgetSearch) }, false).await {
        Ok(op) => op,
        Err(r) => return r,
    };
    let known = me.book().searches.get_mut(&id).filter(|s| !s.forgotten).map(|s| s.forgotten = true).is_some();
    if !known {
        me.violation(format!("a search forgotten that it does not know: {id}"));
        answered(&me, op, StatusCode::NOT_FOUND);
        return StatusCode::NOT_FOUND.into_response();
    }
    answered(&me, op, StatusCode::NO_CONTENT);
    StatusCode::NO_CONTENT.into_response()
}

async fn take_batch(State(me): State<Sharing>, headers: HeaderMap, uri: Uri, axum::Json(body): axum::Json<Value>) -> Response {
    if let Err(r) = keyed(&me, &headers, &uri) {
        return r;
    }
    let user = body["username"].as_str().unwrap_or_default().to_string();
    let batch = body["id"].as_str().unwrap_or_default().to_string();
    let files: Vec<String> = body["files"].as_array().into_iter().flatten().filter_map(|f| f["filename"].as_str().map(str::to_string)).collect();
    let op = Op { user: user.clone(), batch: batch.clone(), files: files.clone(), ..blank(Kind::Batch) };
    let op = match arrive(&me, op, true).await {
        Ok(op) => op,
        Err(r) => return r,
    };
    let refuse = |me: &Sharing, op: Op, why: String| {
        me.violation(why);
        answered(me, op, StatusCode::CONFLICT);
        (StatusCode::CONFLICT, "not expected").into_response()
    };
    if batch.is_empty() || files.is_empty() {
        return refuse(&me, op, format!("a batch with no id or no files, of {user}"));
    }
    if me.book().taken.contains_key(&batch) {
        return refuse(&me, op, format!("batch {batch} asked for twice"));
    }
    let shares: Vec<Share> = me.book().shares.iter().filter(|s| s.user == user).cloned().collect();
    let mut lying = Vec::new();
    for f in &files {
        match shares.iter().find_map(|s| s.lies(f)) {
            Some(p) => lying.push((f.clone(), p)),
            None => return refuse(&me, op, format!("a batch asked of {user} naming a file they do not share: {f}")),
        }
    }
    // Where slskd puts them: the batch's destination under its downloads, else the shared folder's own name.
    let destination = match body["options"]["destination"].as_str() {
        Some(d) => d.to_string(),
        None => files[0].rsplit_once('\\').and_then(|(dir, _)| dir.rsplit('\\').next()).unwrap_or_default().to_string(),
    };
    let to = destination.split(['/', '\\']).filter(|p| !p.is_empty()).fold(me.book().completed.clone(), |at, p| at.join(p));
    let mut transfers = Vec::new();
    for (n, (f, p)) in lying.iter().enumerate() {
        let name = f.rsplit(['\\', '/']).next().unwrap_or(f);
        std::fs::create_dir_all(&to).unwrap();
        let size = std::fs::copy(p, to.join(name)).unwrap();
        transfers.push((format!("{batch}-{n}"), f.clone(), size));
    }
    let listed: Vec<Value> = transfers.iter().map(|t| transfer(&user, t)).collect();
    me.book().taken.insert(batch.clone(), Taken { user: user.clone(), transfers, removed: BTreeSet::new() });
    answered(&me, op, StatusCode::OK);
    axum::Json(json!({ "batch": { "id": batch, "username": user, "transfers": listed }, "failures": [] })).into_response()
}

/// A transfer as slskd lists it: done, whole.
fn transfer(user: &str, (id, filename, size): &(String, String, u64)) -> Value {
    json!({
        "id": id, "username": user, "direction": "Download", "filename": filename, "size": size, "state": "Completed, Succeeded",
        "percentComplete": 100.0, "bytesTransferred": size, "bytesRemaining": 0, "averageSpeed": 1_000_000.0, "exception": null,
    })
}

async fn read_batch(State(me): State<Sharing>, headers: HeaderMap, uri: Uri, Part(id): Part<String>) -> Response {
    if let Err(r) = keyed(&me, &headers, &uri) {
        return r;
    }
    read(&me, &uri);
    let b = me.book();
    match b.taken.get(&id) {
        Some(t) => {
            let listed: Vec<Value> = t.transfers.iter().filter(|x| !t.removed.contains(&x.0)).map(|x| transfer(&t.user, x)).collect();
            axum::Json(json!({ "id": id, "username": t.user, "transfers": listed })).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn downloads(State(me): State<Sharing>, headers: HeaderMap, uri: Uri) -> Response {
    if let Err(r) = keyed(&me, &headers, &uri) {
        return r;
    }
    read(&me, &uri);
    let b = me.book();
    // By user, then by the folder each file is shared in, as slskd lists its downloads.
    let mut users: BTreeMap<String, BTreeMap<String, Vec<Value>>> = BTreeMap::new();
    for t in b.taken.values() {
        for x in t.transfers.iter().filter(|x| !t.removed.contains(&x.0)) {
            let dir = x.1.rsplit_once('\\').map_or("", |(d, _)| d).to_string();
            users.entry(t.user.clone()).or_default().entry(dir).or_default().push(transfer(&t.user, x));
        }
    }
    let listed: Vec<Value> = users
        .into_iter()
        .map(|(user, dirs)| {
            let dirs: Vec<Value> = dirs.into_iter().map(|(dir, files)| json!({ "directory": dir, "fileCount": files.len(), "files": files })).collect();
            json!({ "username": user, "directories": dirs })
        })
        .collect();
    axum::Json(Value::Array(listed)).into_response()
}

async fn drop_transfer(State(me): State<Sharing>, headers: HeaderMap, uri: Uri, Part((user, id)): Part<(String, String)>, Query(q): Query<BTreeMap<String, String>>) -> Response {
    if let Err(r) = keyed(&me, &headers, &uri) {
        return r;
    }
    let batch = me.book().taken.iter().find(|(_, t)| t.user == user && t.transfers.iter().any(|x| x.0 == id)).map(|(b, _)| b.clone());
    let op = match arrive(&me, Op { user: user.clone(), batch: batch.clone().unwrap_or_default(), transfer: id.clone(), ..blank(Kind::DropTransfer) }, false).await {
        Ok(op) => op,
        Err(r) => return r,
    };
    let removed = batch.as_ref().is_some_and(|b| me.book().taken.get(b).is_some_and(|t| t.removed.contains(&id)));
    if batch.is_none() || removed {
        me.violation(format!("a transfer dropped that it does not know: {user} {id}"));
        answered(&me, op, StatusCode::NOT_FOUND);
        return StatusCode::NOT_FOUND.into_response();
    }
    if q.get("remove").map(String::as_str) != Some("true") {
        me.violation(format!("a transfer cancelled but not removed from the list: {user} {id}"));
    }
    if let Some(b) = &batch
        && let Some(t) = me.book().taken.get_mut(b)
    {
        t.removed.insert(id.clone());
    }
    answered(&me, op, StatusCode::NO_CONTENT);
    StatusCode::NO_CONTENT.into_response()
}

async fn unexpected(State(me): State<Sharing>, method: axum::http::Method, uri: Uri) -> Response {
    me.violation(format!("an unexpected call: {method} {uri}"));
    (StatusCode::IM_A_TEAPOT, "not expected").into_response()
}
