//! slskd's REST API (0.9.8, single-track replacement): one search for one track, its answers, and one file fetched.
//! Never bulk (the March 2026 lesson: unattended Soulseek batches put wrong files into the library); the admin picks
//! the file, and the gate (`acoustid`) decides whether it may go in.
//!
//! A fetch goes into its own folder under slskd's completed downloads (a batch's destination, slskd 0.26 or later:
//! `cratarr/<batch>`), so nothing else that watches that folder takes it for its own; Cratarr moves it out to its
//! staging the moment it completes.
//!
//! slskd takes one operation at a time (0.17's round 1.3, SK2): every operation (a download batch or a search started, a
//! transfer dropped, a search forgotten) goes through one lane per slskd address, one after another, whoever asks: Soulseek's
//! records, its searcher, the bulk queue and single-track fetch. Reads go beside the lane, never waiting for it. A 429 is slskd
//! saying it is busy: the operation is tried again in the lane after 1 s, doubling to 30 s, or after its `Retry-After` where that
//! is longer (15 minutes at most), until slskd takes it. A 429 is never anyone's failure. slskd's transfers still run side by side:
//! the lane holds only the asking.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// The folder under slskd's completed downloads that Cratarr's fetches go into.
pub const FOLDER: &str = "cratarr";
/// How long slskd listens for answers (its own default), in milliseconds, as its API counts: 15 ended every search at
/// once with no answers (slskd 0.26, seen 2026-11-06).
const LISTEN_MS: u64 = 15_000;

/// slskd itself could not be asked: it did not answer, would not take Cratarr's key, or says it is not up. Nothing a user
/// did, so nothing to hold against one (`soulseek::fetch_from`, `track_intake::fetch`). Reads as the failure it carries.
#[derive(Debug)]
pub struct Away(String);

impl std::fmt::Display for Away {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Away {}

fn unreached(e: reqwest::Error) -> Away {
    Away(format!("slskd could not be reached: {:#}", anyhow::Error::new(e)))
}

/// slskd busy (SK2): a 429, "Only one concurrent operation is permitted". An operation meeting it is tried again in its lane; a
/// read meeting it (a proxy in front of slskd) is read again at its next look, and is no failed read. Never an item's state, never
/// a user's failure.
#[derive(Debug)]
pub struct Busy(String);

impl std::fmt::Display for Busy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Busy {}

/// An operation's answer lost (SK2.3): it was sent, and slskd's answer never came whole (a timeout, the connection cut, an answer
/// that could not be read). Whether slskd took it is read back by its id; never a user's failure.
#[derive(Debug)]
pub struct Lost(String);

impl std::fmt::Display for Lost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Lost {}

/// slskd does not know what was read (a 404): a batch it forgot, at a restart (SK1.7), or one it never took (SK2.3).
#[derive(Debug)]
pub struct NotKnown(String);

impl std::fmt::Display for NotKnown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NotKnown {}

/// An operation not sent: what asked for it was no longer there when its turn came (SK1.6: the start looks inside the lane).
#[derive(Debug)]
pub struct Gone;

impl std::fmt::Display for Gone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("it left the list before its turn")
    }
}

impl std::error::Error for Gone {}

/// What a user of Soulseek did, as slskd tells it (TN.1): offline, would not send, or failed while slskd itself was well. Held
/// against that user. Reads as what slskd said.
#[derive(Debug)]
pub struct Declined(String);

impl std::fmt::Display for Declined {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Declined {}

/// Whether an answer is slskd's own trouble, whoever was asked for: its key refused, or a proxy saying it is not up. A 429 is
/// slskd busy (`Busy`), asked again.
fn its_own(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 401 | 403 | 502 | 503 | 504)
}

/// The first wait after slskd says it is busy; each next one doubles, to `BUSY_MOST`.
const BUSY_FIRST: Duration = Duration::from_secs(1);
const BUSY_MOST: Duration = Duration::from_secs(30);
/// The longest `Retry-After` heeded (15 minutes, as TN's).
const RETRY_AFTER_MOST: Duration = Duration::from_secs(15 * 60);
/// How often the log says slskd is busy, while it is.
const BUSY_SAID_EVERY: u64 = 60_000;

/// The wait before an operation is tried again, after `tries` busy answers in a row (SK2.2): 1 s, 2 s, 4 s … 30 s at most; its
/// `Retry-After` where that is longer, to 15 minutes. Pure.
pub fn busy_wait(tries: u32, retry_after: Option<Duration>) -> Duration {
    let doubled = BUSY_FIRST.saturating_mul(1 << tries.saturating_sub(1).min(5)).min(BUSY_MOST);
    retry_after.map_or(doubled, |r| r.min(RETRY_AFTER_MOST).max(doubled))
}

/// Time as a lane waits it: the runtime's own, or a test's (`Clock::stopped`), which moves only when it is waited on.
#[derive(Clone, Default)]
pub struct Clock(Option<Arc<AtomicU64>>);

impl Clock {
    /// A clock that stands still until waited on: each wait moves it on at once (the tests' own time).
    #[cfg(any(test, feature = "test-support"))]
    pub fn stopped() -> Self {
        Self(Some(Arc::new(AtomicU64::new(0))))
    }

    /// Milliseconds on this clock.
    pub fn ms(&self) -> u64 {
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        match &self.0 {
            Some(t) => t.load(Ordering::SeqCst),
            None => START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64,
        }
    }

    pub async fn sleep(&self, d: Duration) {
        match &self.0 {
            Some(t) => {
                t.fetch_add(d.as_millis() as u64, Ordering::SeqCst);
                tokio::task::yield_now().await;
            }
            None => tokio::time::sleep(d).await,
        }
    }
}

/// One slskd address's lane (SK2): its operations one at a time, and when the log last said it was busy.
#[derive(Default)]
struct Lane {
    turn: tokio::sync::Mutex<()>,
    /// The clock's milliseconds plus one when the log last said slskd was busy; 0: never.
    said_at: AtomicU64,
    /// How many times the log said so (the tests read it).
    said: AtomicU32,
}

/// The lane of slskd at `url`: one for every client of that address, whoever made it.
fn lane_of(url: &str) -> Arc<Lane> {
    static LANES: std::sync::Mutex<BTreeMap<String, Arc<Lane>>> = std::sync::Mutex::new(BTreeMap::new());
    LANES.lock().unwrap_or_else(|e| e.into_inner()).entry(url.to_string()).or_default().clone()
}

/// slskd at an address, with its API key.
#[derive(Clone)]
pub struct Slskd {
    url: String,
    key: String,
    http: reqwest::Client,
    clock: Clock,
    /// Tests: slskd answered within this process by a router of the test's own (`within`): no socket, so tokio's paused clock does
    /// not run on past a question while it is answered.
    #[cfg(any(test, feature = "test-support"))]
    within: Option<axum::Router>,
}

/// One file offered in answer to a search.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Offered {
    pub filename: String,
    pub size: i64,
    #[serde(default)]
    pub extension: Option<String>,
    #[serde(default)]
    pub bit_rate: Option<i64>,
    #[serde(default)]
    pub bit_depth: Option<i64>,
    #[serde(default)]
    pub sample_rate: Option<i64>,
    /// Seconds.
    #[serde(default)]
    pub length: Option<i64>,
    #[serde(default)]
    pub is_locked: bool,
}

/// One user's answer.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Answer {
    pub username: String,
    #[serde(default)]
    pub files: Vec<Offered>,
    #[serde(default)]
    pub has_free_upload_slot: bool,
    #[serde(default)]
    pub queue_length: i64,
    #[serde(default)]
    pub upload_speed: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Search {
    #[serde(default)]
    is_complete: bool,
}

/// A fetch's transfer as slskd keeps it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transfer {
    pub id: String,
    /// The file as its user shares it.
    #[serde(default)]
    pub filename: String,
    /// slskd's states as words ("Queued, Remotely", "InProgress", "Completed, Succeeded"…).
    pub state: String,
    #[serde(default)]
    pub percent_complete: f64,
    #[serde(default)]
    pub exception: Option<String>,
}

impl Transfer {
    pub fn done(&self) -> bool {
        self.state.contains("Completed")
    }

    pub fn succeeded(&self) -> bool {
        self.state.contains("Succeeded")
    }
}

#[derive(Deserialize)]
struct Batch {
    #[serde(default)]
    transfers: Option<Vec<Transfer>>,
}

#[derive(Deserialize)]
struct Enqueued {
    batch: Option<Batch>,
    #[serde(default)]
    failures: Vec<Failure>,
}

#[derive(Deserialize)]
struct Failure {
    #[serde(default)]
    message: String,
}

impl Slskd {
    pub fn new(url: &str, key: &str) -> Result<Self> {
        // Never a redirect (O1): its key stays on the host given.
        let http = crate::app::reach::lan_client().timeout(Duration::from_secs(20)).build()?;
        Ok(Self {
            url: url.trim_end_matches('/').to_string(),
            key: key.to_string(),
            http,
            clock: Clock::default(),
            #[cfg(any(test, feature = "test-support"))]
            within: None,
        })
    }

    /// Tests: slskd at `url` (its lane that address's) answered within this process by `router`, its routes as slskd's API has them.
    #[cfg(any(test, feature = "test-support"))]
    pub fn within(url: &str, key: &str, router: axum::Router) -> Self {
        Self { within: Some(router), ..Self::new(url, key).unwrap() }
    }

    /// A request sent to slskd (in tests, to the router standing in for it).
    async fn sent(&self, request: reqwest::RequestBuilder) -> reqwest::Result<reqwest::Response> {
        #[cfg(any(test, feature = "test-support"))]
        if let Some(router) = self.within.clone() {
            return Ok(answered(router, request.build()?).await);
        }
        request.send().await
    }

    /// This client, its lane's waits on `clock` (the tests' own time).
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    /// How many times the log said this slskd was busy (SK2.2 (e): once a minute while it is).
    #[cfg(any(test, feature = "test-support"))]
    pub fn busy_said(&self) -> u32 {
        lane_of(&self.url).said.load(Ordering::SeqCst)
    }

    fn at(&self, path: &str) -> String {
        format!("{}/api/v0/{path}", self.url)
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let r = self.sent(self.http.get(self.at(path)).header("X-API-Key", &self.key)).await.map_err(unreached)?;
        let status = r.status();
        if status.as_u16() == 429 {
            return Err(Busy("slskd is busy".into()).into());
        }
        if its_own(status) {
            return Err(Away(format!("slskd answered {status}")).into());
        }
        if status.as_u16() == 404 {
            return Err(NotKnown(format!("slskd answered {status}")).into());
        }
        if !status.is_success() {
            bail!("slskd answered {status}");
        }
        r.json().await.context("slskd's answer could not be read")
    }

    /// An operation through its address's lane (SK2), the request made by `send` for each try. slskd busy (a 429) is asked
    /// again in the lane after `busy_wait`, until it takes it, the log saying so once a minute; `still` is asked inside the lane
    /// before each try, and where it says no, nothing is sent (`Gone`). Its answer, any but a 429; a request that never reached
    /// slskd is `Away`, one whose answer did not come `Lost`.
    async fn operate(&self, send: impl Fn() -> reqwest::RequestBuilder, still: impl Fn() -> bool) -> Result<reqwest::Response> {
        let lane = lane_of(&self.url);
        let _turn = lane.turn.lock().await;
        let mut tries = 0;
        loop {
            if !still() {
                return Err(Gone.into());
            }
            let r = match self.sent(send().header("X-API-Key", &self.key)).await {
                Ok(r) => r,
                Err(e) if e.is_connect() => return Err(unreached(e).into()),
                Err(e) => return Err(Lost(format!("slskd's answer did not come: {:#}", anyhow::Error::new(e))).into()),
            };
            if r.status().as_u16() != 429 {
                return Ok(r);
            }
            tries += 1;
            let after = r.headers().get(reqwest::header::RETRY_AFTER).and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse::<u64>().ok()).map(Duration::from_secs);
            let now = self.clock.ms();
            let said = lane.said_at.load(Ordering::SeqCst);
            if said == 0 || now + 1 >= said + BUSY_SAID_EVERY {
                lane.said_at.store(now + 1, Ordering::SeqCst);
                lane.said.fetch_add(1, Ordering::SeqCst);
                tracing::info!(target: "soulseek", slskd = %self.url, tries, "slskd is busy with another operation: asked again shortly");
            }
            self.clock.sleep(busy_wait(tries, after)).await;
        }
    }

    /// Every download slskd remembers, by the user it came from: each file's name as that user shares it (the intake
    /// card's "where from", 0.14).
    pub async fn downloads(&self) -> Result<Vec<(String, Vec<String>)>> {
        let users: serde_json::Value = self.get("transfers/downloads").await?;
        Ok(users
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|u| {
                        let files = u["directories"].as_array().into_iter().flatten().flat_map(|d| d["files"].as_array().into_iter().flatten()).filter_map(|f| f["filename"].as_str().map(str::to_string)).collect();
                        (u["username"].as_str().unwrap_or_default().to_string(), files)
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Its version, which also says it answers and the key works.
    pub async fn version(&self) -> Result<String> {
        self.get("application/version").await
    }

    /// Starts a search; its id.
    pub async fn search(&self, text: &str) -> Result<String> {
        let id = uuid();
        let body = json!({ "id": id, "searchText": text, "searchTimeout": LISTEN_MS, "filterResponses": true, "minimumResponseFileCount": 1 });
        // A search whose answer was lost is searched again later, as with slskd away: no miss.
        let r = self.operate(|| self.http.post(self.at("searches")).json(&body), || true).await.map_err(|e| if e.is::<Lost>() { Away(format!("{e:#}")).into() } else { e })?;
        let status = r.status();
        if !status.is_success() {
            let said = format!("slskd would not search: {status} {}", r.text().await.unwrap_or_default());
            return Err(self.turned_down(status, said).await);
        }
        Ok(id)
    }

    /// Whether the search is over; its answers are read only after (they are not all there before).
    /// slskd busy (a 429 from a proxy before it): not yet (the review of part 2, M2).
    pub async fn searched(&self, id: &str) -> Result<bool> {
        match self.get::<Search>(&format!("searches/{id}")).await {
            Ok(s) => Ok(s.is_complete),
            Err(e) if e.is::<Busy>() => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// A search's answers. slskd busy: read again (`busy_wait`), for two minutes at most (M2).
    pub async fn answers(&self, id: &str) -> Result<Vec<Answer>> {
        let (began, mut tries) = (self.clock.ms(), 0);
        loop {
            match self.get(&format!("searches/{id}/responses")).await {
                Err(e) if e.is::<Busy>() && self.clock.ms() < began + 120_000 => {
                    tries += 1;
                    self.clock.sleep(busy_wait(tries, None)).await;
                }
                other => return other,
            }
        }
    }

    /// What slskd saying no comes to. A 500 is how it says both "that user failed" and "I am not on Soulseek": there it is
    /// asked which, as a slskd just back and not signed in yet would otherwise fail every user in a moment.
    async fn turned_down(&self, status: reqwest::StatusCode, said: String) -> anyhow::Error {
        if its_own(status) || self.off_soulseek().await { Away(said).into() } else { Declined(said).into() }
    }

    /// Whether slskd says it is not signed in to Soulseek, or cannot answer now (away, or paused: TN.1's class). Its word as far as
    /// it goes: any other shape of answer says nothing, and what it said stands.
    async fn off_soulseek(&self) -> bool {
        match self.get::<serde_json::Value>("application").await {
            Ok(v) => v["server"]["isLoggedIn"] == false,
            Err(e) => matches!(crate::app::turn::class(&e), crate::app::turn::Failure::Away(_) | crate::app::turn::Failure::Paused { .. }),
        }
    }

    /// The search gone from slskd's list (through the lane).
    pub async fn forget(&self, id: &str) {
        let _ = self.operate(|| self.http.delete(self.at(&format!("searches/{id}"))), || true).await;
    }

    /// Asks `user` for one file, into its own folder (`cratarr/<batch>`); the batch's id, which names that folder. Through the
    /// lane. An answer lost is read back by the batch's id, made before it was sent (SK2.3's, for one file): slskd has it, it is
    /// under way; slskd does not know it, it is sent again under that id, once; not read, slskd's trouble (`Away`). Never two
    /// batches of one file.
    pub async fn fetch(&self, user: &str, file: &Offered) -> Result<String> {
        let batch = uuid();
        let body = json!({
            "id": batch,
            "username": user,
            "files": [{ "filename": file.filename, "size": file.size }],
            "options": { "destination": format!("{FOLDER}/{batch}") },
        });
        let mut again = true;
        let r = loop {
            match self.operate(|| self.http.post(self.at("transfers/downloads/batches")).json(&body), || true).await {
                Err(e) if e.is::<Lost>() => match self.transfers(&batch).await {
                    Ok(_) => return Ok(batch),
                    Err(r) if r.is::<NotKnown>() && again => again = false,
                    Err(_) => return Err(Away(format!("{e:#}")).into()),
                },
                other => break other?,
            }
        };
        let status = r.status();
        if status.as_u16() == 404 {
            return Err(Declined(format!("{user} is offline")).into());
        }
        if !status.is_success() {
            let said = format!("slskd would not fetch it: {status} {}", r.text().await.unwrap_or_default());
            return Err(self.turned_down(status, said).await);
        }
        let e: Enqueued = r.json().await.context("slskd's answer could not be read")?;
        if let Some(f) = e.failures.first() {
            return Err(Declined(format!("slskd would not fetch it: {}", f.message)).into());
        }
        if e.batch.is_none() {
            bail!("slskd did not take the fetch");
        }
        Ok(batch)
    }

    /// Asks `user` for a whole folder's files in one batch under the id `batch` (`new_batch`, kept on its item before it is sent:
    /// SK2.3), into its own folder (`cratarr/<batch>`): a record fetched whole (0.15, Soulseek in Cratarr). Through the lane,
    /// `still` asked before each try (`operate`). An answer that did not come, or came unreadable, is `Lost`: read the batch back.
    pub async fn fetch_many(&self, batch: &str, user: &str, files: &[Offered], still: impl Fn() -> bool) -> Result<()> {
        let listed: Vec<serde_json::Value> = files.iter().map(|f| json!({ "filename": f.filename, "size": f.size })).collect();
        let body = json!({ "id": batch, "username": user, "files": listed, "options": { "destination": format!("{FOLDER}/{batch}") } });
        let r = self.operate(|| self.http.post(self.at("transfers/downloads/batches")).json(&body), still).await?;
        let status = r.status();
        if status.as_u16() == 404 {
            return Err(Declined(format!("{user} is offline")).into());
        }
        if !status.is_success() {
            let said = format!("slskd would not fetch them: {status} {}", r.text().await.unwrap_or_default());
            return Err(self.turned_down(status, said).await);
        }
        let e: Enqueued = r.json().await.map_err(|e| Lost(format!("slskd's answer could not be read: {:#}", anyhow::Error::new(e))))?;
        if let Some(f) = e.failures.first() {
            return Err(Declined(format!("{user} would not send them: {}", f.message)).into());
        }
        if e.batch.is_none() {
            bail!("slskd did not take the fetch");
        }
        Ok(())
    }

    /// A batch's transfers, as they are now. One slskd does not know is `NotKnown`.
    pub async fn transfers(&self, batch: &str) -> Result<Vec<Transfer>> {
        let b: Batch = self.get(&format!("transfers/downloads/batches/{batch}")).await?;
        Ok(b.transfers.unwrap_or_default())
    }

    /// The fetch's transfer, as it is now.
    pub async fn transfer(&self, batch: &str) -> Result<Transfer> {
        let b: Batch = self.get(&format!("transfers/downloads/batches/{batch}")).await?;
        b.transfers.and_then(|t| t.into_iter().next()).context("slskd has no transfer for it")
    }

    /// The transfer gone from slskd's list (and stopped, when it still runs), through the lane.
    pub async fn drop_transfer(&self, user: &str, id: &str) {
        let path = format!("transfers/downloads/{}/{id}?remove=true", segment(user));
        let _ = self.operate(|| self.http.delete(self.at(&path)), || true).await;
    }
}

/// A new batch's id (a UUID, as slskd takes ids), made before its batch is asked for and kept on its item (SK2.3): an answer lost
/// is read back by it, and a batch is never asked for twice.
pub fn new_batch() -> String {
    uuid()
}

/// Tests: `r` answered by `router` within this process, as reqwest would have had it from slskd.
#[cfg(any(test, feature = "test-support"))]
async fn answered(router: axum::Router, r: reqwest::Request) -> reqwest::Response {
    use tower::ServiceExt as _;
    let mut asked = axum::http::Request::builder().method(r.method().clone()).uri(r.url().as_str());
    for (name, value) in r.headers() {
        asked = asked.header(name, value);
    }
    let body = r.body().and_then(reqwest::Body::as_bytes).map(<[u8]>::to_vec).unwrap_or_default();
    let answer = router.oneshot(asked.body(axum::body::Body::from(body)).unwrap()).await.unwrap();
    let (parts, body) = answer.into_parts();
    let body = axum::body::to_bytes(body, usize::MAX).await.unwrap();
    reqwest::Response::from(axum::http::Response::from_parts(parts, body))
}

/// A random (version 4) UUID, as slskd takes ids.
fn uuid() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("the operating system's random source");
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

/// Whether a folder's name is a batch's id (a UUID): one of Cratarr's own fetches, under [`FOLDER`].
pub fn is_batch(name: &str) -> bool {
    name.len() == 36 && name.char_indices().all(|(i, c)| if matches!(i, 8 | 13 | 18 | 23) { c == '-' } else { c.is_ascii_hexdigit() })
}

/// A user's name as one part of a path.
fn segment(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// A file's name without the uploader's folders (Soulseek names files with backslashes).
pub fn base_name(filename: &str) -> &str {
    filename.rsplit(['\\', '/']).next().unwrap_or(filename)
}

/// One file as the page is offered it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: usize,
    pub user: String,
    pub file: String,
    pub format: String,
    pub lossless: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_depth: Option<i64>,
    pub size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length_ms: Option<i64>,
    pub free_slot: bool,
    pub queue: i64,
    pub speed: i64,
    #[serde(skip)]
    pub offered: Offered,
}

/// How far a file's length may be from the recording's (the owner's gate).
pub const LENGTH_SLACK_MS: i64 = 2_000;

/// The answers as candidates for a recording of `length_ms`: audio Cratarr plays, not locked, of about its length
/// (where both are known); lossless first, then a free slot, a short queue, the fastest.
pub fn candidates(answers: Vec<Answer>, length_ms: Option<i64>) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    for a in answers {
        for f in a.files {
            if f.is_locked {
                continue;
            }
            let ext = f.extension.clone().filter(|e| !e.is_empty()).unwrap_or_else(|| base_name(&f.filename).rsplit('.').next().unwrap_or("").to_string()).to_lowercase();
            let lossless = matches!(ext.as_str(), "flac" | "wav" | "aiff" | "aif" | "alac");
            if !lossless && !matches!(ext.as_str(), "mp3" | "m4a" | "ogg" | "opus") {
                continue;
            }
            let file_ms = f.length.map(|s| s * 1000);
            if let (Some(want), Some(got)) = (length_ms, file_ms)
                && (want - got).abs() > LENGTH_SLACK_MS
            {
                continue;
            }
            out.push(Candidate {
                id: 0,
                user: a.username.clone(),
                file: f.filename.clone(),
                format: ext,
                lossless,
                bitrate: f.bit_rate,
                sample_rate: f.sample_rate,
                bit_depth: f.bit_depth,
                size: f.size,
                length_ms: file_ms,
                free_slot: a.has_free_upload_slot,
                queue: a.queue_length,
                speed: a.upload_speed,
                offered: f,
            });
        }
    }
    out.sort_by(|a, b| {
        b.lossless
            .cmp(&a.lossless)
            .then(b.free_slot.cmp(&a.free_slot))
            .then(a.queue.cmp(&b.queue))
            .then(b.bitrate.unwrap_or(0).cmp(&a.bitrate.unwrap_or(0)))
            .then(b.speed.cmp(&a.speed))
    });
    for (i, c) in out.iter_mut().enumerate() {
        c.id = i;
    }
    out
}

/// slskd, scripted (SCENARIOS-SK17, "Shapes and fakes"): it acts as slskd does, taking one operation at a time. An operation that
/// arrives while another is still being answered gets 429, "Only one concurrent operation is permitted", and that is a violation
/// (Cratarr caused it), but where slskd is busy with work of its own (`busy`, a lost answer's second). Every operation, read and
/// DELETE is recorded, with when it began on the fake's clock; a call it does not expect is a violation, answered with an error.
/// The tests of the crates above script it too (test-support).
#[cfg(any(test, feature = "test-support"))]
#[allow(dead_code)]
pub mod scripted {
    use super::*;
    use axum::extract::{Path as Part, State as Held};
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};
    use std::collections::BTreeSet;
    use std::sync::Mutex;

    /// What an operation asked for.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Kind {
        Batch,
        Search,
        DropTransfer,
        ForgetSearch,
    }

    /// One operation as it reached slskd: what, of whom, its batch, when it began (ms on the fake's clock), and its answer.
    #[derive(Debug, Clone)]
    pub struct Op {
        pub kind: Kind,
        pub user: String,
        pub batch: String,
        pub files: Vec<String>,
        pub began_ms: u64,
        pub status: u16,
    }

    #[derive(Debug, Clone, PartialEq)]
    enum Now {
        Going(f64),
        Succeeded,
        Errored(String),
        Forgotten,
    }

    #[derive(Debug, Clone)]
    struct Batch {
        user: String,
        files: Vec<String>,
        now: Now,
        dropped: bool,
        /// A record's (`singles` says which are not): only records' batches meet the one-user rule (SC02.9).
        record: bool,
    }

    impl Batch {
        fn done(&self) -> bool {
            self.dropped || !matches!(self.now, Now::Going(_))
        }
    }

    type Hook = Arc<dyn Fn(&Op) + Send + Sync>;

    #[derive(Default)]
    struct Script {
        slow_ms: u64,
        busy: u32,
        busy_until_ms: u64,
        retry_after: Option<u64>,
        lose: u32,
        lose_untaken: bool,
        hold: Option<f64>,
        offline: BTreeSet<String>,
        read_busy: u32,
        read_ok: u32,
        responses_busy: u32,
        singles: BTreeSet<String>,
        in_flight: u32,
        ops: Vec<Op>,
        reads: Vec<(String, bool)>,
        batches: BTreeMap<String, Batch>,
        violations: Vec<String>,
        answers: Vec<serde_json::Value>,
        writes_to: Option<std::path::PathBuf>,
        content: Option<std::path::PathBuf>,
        on_post: Option<Hook>,
        clock: Clock,
    }

    /// The fake: its address, its script, and its clock (the tests' own, `Clock::stopped`, unless `real_clock`).
    #[derive(Clone)]
    pub struct Scripted {
        s: Arc<Mutex<Script>>,
        url: String,
    }

    const ONE_AT_A_TIME: &str = "Only one concurrent operation is permitted. Wait until the previous operation completes";

    impl Scripted {
        /// A fake listening on this machine, taking batches whole at once (`hold_all` keeps them going). In a runtime.
        pub fn new() -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let me = Self { s: Arc::new(Mutex::new(Script { clock: Clock::stopped(), ..Script::default() })), url };
            let app = axum::Router::new()
                .route("/api/v0/transfers/downloads/batches", axum::routing::post(take_batch))
                .route("/api/v0/transfers/downloads/batches/{id}", axum::routing::get(read_batch))
                .route("/api/v0/transfers/downloads/{user}/{id}", axum::routing::delete(drop_transfer))
                .route("/api/v0/searches", axum::routing::post(take_search))
                .route("/api/v0/searches/{id}", axum::routing::get(searched).delete(forget_search))
                .route("/api/v0/searches/{id}/responses", axum::routing::get(responses))
                .route("/api/v0/application", axum::routing::get(|| async { axum::Json(json!({ "server": { "isLoggedIn": true } })) }))
                .fallback(unexpected)
                .with_state(me.clone());
            crate::run::spawn(async move { axum::serve(tokio::net::TcpListener::from_std(listener).unwrap(), app).await.unwrap() });
            me
        }

        fn with(self, f: impl FnOnce(&mut Script)) -> Self {
            f(&mut self.s.lock().unwrap());
            self
        }

        /// Each operation answered after `ms` (real time).
        pub fn slow(self, ms: u64) -> Self {
            self.with(|s| s.slow_ms = ms)
        }
        /// The next `n` operations answered 429, slskd busy with something not Cratarr's (no violation).
        pub fn busy(self, n: u32) -> Self {
            self.with(|s| s.busy = n)
        }
        /// Every operation answered 429 until `ms` on the fake's clock (no violation).
        pub fn busy_for(self, ms: u64) -> Self {
            self.with(|s| s.busy_until_ms = ms)
        }
        /// Such a 429 carries `Retry-After`.
        pub fn retry_after(self, secs: u64) -> Self {
            self.with(|s| s.retry_after = Some(secs))
        }
        /// The next batch asked for is taken, its connection cut without an answer, and slskd busy a second after (its own work).
        pub fn lose_answer_of_first_post(self) -> Self {
            self.with(|s| s.lose = 1)
        }
        /// As `lose_answer_of_first_post`, the connection cut before slskd took the batch (SK2.3 (a)).
        pub fn cut_before_taking(self) -> Self {
            self.with(|s| (s.lose, s.lose_untaken) = (1, true))
        }
        /// Every batch kept going at `percent` until the test lets it end.
        pub fn hold_all(self, percent: f64) -> Self {
            self.with(|s| s.hold = Some(percent))
        }
        /// A batch asked of `user` answered 404: the user is offline.
        pub fn offline(self, user: &str) -> Self {
            self.with(|s| {
                s.offline.insert(user.to_string());
            })
        }
        /// From now, a batch asked of `user` answered 404: the user is offline.
        pub fn then_offline(&self, user: &str) {
            self.s.lock().unwrap().offline.insert(user.to_string());
        }
        /// From now, the next `n` operations answered 429 (no violation).
        pub fn then_busy(&self, n: u32) {
            self.s.lock().unwrap().busy = n;
        }
        /// The next `n` reads answered 429 (a proxy in front of slskd).
        pub fn read_busy(self, n: u32) -> Self {
            self.with(|s| s.read_busy = n)
        }
        /// From now, the next `ok` reads answered, then `n` answered 429.
        pub fn then_reads_busy(&self, ok: u32, n: u32) {
            let mut s = self.s.lock().unwrap();
            (s.read_ok, s.read_busy) = (ok, n);
        }
        /// slskd holding `batch` of `user` already (taken before the test began: a restart's), going at `percent`, with `files`.
        pub fn knows(self, batch: &str, user: &str, files: &[String], percent: f64) -> Self {
            self.with(|s| {
                s.batches.insert(batch.to_string(), Batch { user: user.to_string(), files: files.to_vec(), now: Now::Going(percent), dropped: false, record: true });
            })
        }
        /// A batch of these files alone is the bulk queue's or single-track fetch's, not a record's: the one-user rule (Soulseek's
        /// records' alone, SC02.9) neither counts it nor holds it.
        pub fn singles(self, files: &[&str]) -> Self {
            self.with(|s| s.singles = files.iter().map(|f| f.to_string()).collect())
        }
        /// The next `n` reads of a search's answers answered 429.
        pub fn responses_busy(self, n: u32) -> Self {
            self.with(|s| s.responses_busy = n)
        }
        /// A search's answers.
        pub fn answers(self, answers: Vec<serde_json::Value>) -> Self {
            self.with(|s| s.answers = answers)
        }
        /// A batch's files written under `folder`'s `cratarr/<batch>` the moment it ends well: each a copy of `content`, else "x".
        pub fn writes_to(self, folder: &std::path::Path, content: Option<&std::path::Path>) -> Self {
            self.with(|s| (s.writes_to, s.content) = (Some(folder.to_path_buf()), content.map(std::path::Path::to_path_buf)))
        }
        /// The runtime's own clock for the lane's waits, not the fake's.
        pub fn real_clock(self) -> Self {
            self.with(|s| s.clock = Clock::default())
        }
        /// `f` called with each batch asked for, as it arrives, before it is answered.
        pub fn on_post(self, f: impl Fn(&Op) + Send + Sync + 'static) -> Self {
            self.with(|s| s.on_post = Some(Arc::new(f)))
        }

        /// A client of this slskd, its lane on the fake's clock.
        pub fn client(&self) -> Slskd {
            Slskd::new(&self.url, "key").unwrap().with_clock(self.clock())
        }

        pub fn clock(&self) -> Clock {
            self.s.lock().unwrap().clock.clone()
        }

        pub fn ops(&self) -> Vec<Op> {
            self.s.lock().unwrap().ops.clone()
        }
        /// Every batch asked for, each try, in order.
        pub fn posts(&self) -> Vec<Op> {
            self.ops().into_iter().filter(|o| o.kind == Kind::Batch).collect()
        }
        pub fn post_users(&self) -> Vec<String> {
            self.posts().into_iter().map(|o| o.user).collect()
        }
        /// When each batch was asked for, in whole seconds of the fake's clock, from the first.
        pub fn post_offsets_secs(&self) -> Vec<u64> {
            let p = self.posts();
            let first = p.first().map_or(0, |o| o.began_ms);
            p.iter().map(|o| (o.began_ms - first) / 1000).collect()
        }
        /// Whether no operation ever arrived while another was answered.
        pub fn never_overlapped(&self) -> bool {
            !self.s.lock().unwrap().violations.iter().any(|v| v.starts_with("an operation while"))
        }
        pub fn violations(&self) -> Vec<String> {
            self.s.lock().unwrap().violations.clone()
        }
        /// Reads that came while an operation was being answered.
        pub fn reads_during_operations(&self) -> usize {
            self.s.lock().unwrap().reads.iter().filter(|(_, during)| *during).count()
        }
        /// The batches slskd took for `user`, in order.
        pub fn batches_of(&self, user: &str) -> Vec<String> {
            let s = self.s.lock().unwrap();
            let mut out: Vec<String> = Vec::new();
            for o in s.ops.iter().filter(|o| o.kind == Kind::Batch && o.user == user && s.batches.contains_key(&o.batch)) {
                if !out.contains(&o.batch) {
                    out.push(o.batch.clone());
                }
            }
            out
        }
        /// Reads of a batch.
        pub fn reads_of(&self, batch: &str) -> usize {
            let at = format!("batches/{batch}");
            self.s.lock().unwrap().reads.iter().filter(|(p, _)| p.ends_with(&at)).count()
        }
        /// Whether a transfer of `batch` was dropped.
        pub fn deleted(&self, batch: &str) -> bool {
            self.s.lock().unwrap().batches.get(batch).is_some_and(|b| b.dropped)
        }
        /// Every DELETE of a transfer, of whom.
        pub fn drops(&self) -> Vec<String> {
            self.ops().into_iter().filter(|o| o.kind == Kind::DropTransfer).map(|o| o.user).collect()
        }
        /// The batches still going.
        pub fn going(&self) -> Vec<String> {
            self.s.lock().unwrap().batches.iter().filter(|(_, b)| !b.done()).map(|(id, _)| id.clone()).collect()
        }

        fn end(&self, batch: &str, now: Now) {
            let (write, files) = {
                let mut s = self.s.lock().unwrap();
                let write = s.writes_to.clone().zip(Some(s.content.clone()));
                let Some(b) = s.batches.get_mut(batch) else { panic!("no batch {batch}") };
                b.now = now.clone();
                (write, b.files.clone())
            };
            if now == Now::Succeeded
                && let Some((folder, content)) = write
            {
                write_files(&folder, batch, &files, content.as_deref());
            }
        }
        /// The batch ends well, its files written.
        pub fn release(&self, batch: &str) {
            self.end(batch, Now::Succeeded);
        }
        /// The batch's transfers end in an error, with slskd's words.
        pub fn error(&self, batch: &str, said: &str) {
            self.end(batch, Now::Errored(said.to_string()));
        }
        /// slskd forgets the batch (a restart): it answers 404 for it.
        pub fn forget(&self, batch: &str) {
            self.end(batch, Now::Forgotten);
        }
        /// The batch at `percent`.
        pub fn at(&self, batch: &str, percent: f64) {
            self.end(batch, Now::Going(percent));
        }
    }

    fn write_files(folder: &std::path::Path, batch: &str, files: &[String], content: Option<&std::path::Path>) {
        let dir = folder.join(FOLDER).join(batch);
        for f in files {
            let at = dir.join(base_name(f));
            std::fs::create_dir_all(&dir).unwrap();
            match content {
                Some(c) => std::fs::copy(c, &at).map(|_| ()).unwrap(),
                None => std::fs::write(&at, b"x").unwrap(),
            }
        }
    }

    /// An operation arriving: whether slskd takes it now, else its 429. Counted in flight while taken.
    fn arrive(me: &Scripted, mut op: Op) -> Result<(Op, Option<Hook>, u64), Response> {
        let mut s = me.s.lock().unwrap();
        let now = s.clock.ms();
        op.began_ms = now;
        let busy = if s.busy > 0 {
            s.busy -= 1;
            true
        } else {
            now < s.busy_until_ms
        };
        if busy || s.in_flight > 0 {
            if !busy {
                s.violations.push(format!("an operation while another was answered: {:?} of {}", op.kind, op.user));
            }
            op.status = 429;
            let after = s.retry_after;
            s.ops.push(op);
            let mut r = (StatusCode::TOO_MANY_REQUESTS, ONE_AT_A_TIME).into_response();
            if let Some(a) = after {
                r.headers_mut().insert("retry-after", a.to_string().parse().unwrap());
            }
            return Err(r);
        }
        s.in_flight += 1;
        Ok((op, s.on_post.clone(), s.slow_ms))
    }

    async fn take_batch(Held(me): Held<Scripted>, axum::Json(b): axum::Json<serde_json::Value>) -> Response {
        let user = b["username"].as_str().unwrap_or_default().to_string();
        let batch = b["id"].as_str().unwrap_or_default().to_string();
        let files: Vec<String> = b["files"].as_array().into_iter().flatten().filter_map(|f| f["filename"].as_str().map(str::to_string)).collect();
        let op = Op { kind: Kind::Batch, user: user.clone(), batch: batch.clone(), files: files.clone(), began_ms: 0, status: 0 };
        let hook = me.s.lock().unwrap().on_post.clone();
        if let Some(h) = hook {
            h(&op);
        }
        let (mut op, _, slow) = match arrive(&me, op) {
            Ok(x) => x,
            Err(r) => return r,
        };
        if slow > 0 {
            tokio::time::sleep(Duration::from_millis(slow)).await;
        }
        let (status, write, cut) = {
            let mut s = me.s.lock().unwrap();
            s.in_flight -= 1;
            let record = !files.iter().all(|f| s.singles.contains(f));
            let live = record && s.batches.values().any(|x| x.user == user && x.record && !x.done());
            let status = if s.offline.contains(&user) {
                404
            } else if live {
                s.violations.push(format!("a batch asked of {user} while their earlier one still has a transfer not done"));
                409
            } else if s.batches.contains_key(&batch) {
                s.violations.push(format!("batch {batch} asked for twice"));
                409
            } else {
                200
            };
            let cut = status == 200 && s.lose > 0;
            let taken = status == 200 && !(cut && s.lose_untaken);
            if cut {
                s.lose -= 1;
                s.busy_until_ms = s.clock.ms() + 1000;
            }
            let mut write = None;
            if taken {
                let now = s.hold.map_or(Now::Succeeded, Now::Going);
                if now == Now::Succeeded {
                    write = s.writes_to.clone().map(|w| (w, s.content.clone()));
                }
                s.batches.insert(batch.clone(), Batch { user: user.clone(), files: files.clone(), now, dropped: false, record });
            }
            op.status = if cut { 0 } else { status };
            s.ops.push(op);
            (status, write, cut)
        };
        if let Some((folder, content)) = write {
            write_files(&folder, &batch, &files, content.as_deref());
        }
        if cut {
            // The connection cut without an answer: the task serving it ends here.
            panic!("the fake cuts the connection of batch {batch} without answering");
        }
        match status {
            200 => axum::Json(json!({ "batch": { "id": batch, "transfers": [] }, "failures": [] })).into_response(),
            404 => StatusCode::NOT_FOUND.into_response(),
            _ => (StatusCode::CONFLICT, "not expected").into_response(),
        }
    }

    fn read(me: &Scripted, path: String) -> Option<Response> {
        let mut s = me.s.lock().unwrap();
        let during = s.in_flight > 0;
        s.reads.push((path, during));
        if s.read_ok > 0 {
            s.read_ok -= 1;
            return None;
        }
        if s.read_busy > 0 {
            s.read_busy -= 1;
            return Some((StatusCode::TOO_MANY_REQUESTS, ONE_AT_A_TIME).into_response());
        }
        None
    }

    async fn read_batch(Held(me): Held<Scripted>, Part(id): Part<String>) -> Response {
        if let Some(r) = read(&me, format!("transfers/downloads/batches/{id}")) {
            return r;
        }
        let s = me.s.lock().unwrap();
        let Some(b) = s.batches.get(&id).filter(|b| b.now != Now::Forgotten) else { return StatusCode::NOT_FOUND.into_response() };
        let transfers: Vec<serde_json::Value> = b
            .files
            .iter()
            .enumerate()
            .map(|(n, f)| {
                let (state, percent, exception) = match &b.now {
                    _ if b.dropped => ("Completed, Cancelled".to_string(), 0.0, None),
                    Now::Going(p) => ((if *p > 0.0 { "InProgress" } else { "Queued, Remotely" }).to_string(), *p, None),
                    Now::Succeeded => ("Completed, Succeeded".to_string(), 100.0, None),
                    Now::Errored(said) => ("Completed, Errored".to_string(), 40.0, Some(said.clone())),
                    Now::Forgotten => unreachable!(),
                };
                json!({ "id": format!("{id}-{n}"), "filename": f, "state": state, "percentComplete": percent, "exception": exception })
            })
            .collect();
        axum::Json(json!({ "id": id, "transfers": transfers })).into_response()
    }

    /// A small operation (a DELETE, a search): taken one at a time, answered after `slow`.
    async fn small(me: &Scripted, kind: Kind, user: String, batch: String) -> Result<(), Response> {
        let (op, _, slow) = arrive(me, Op { kind, user, batch, files: Vec::new(), began_ms: 0, status: 0 })?;
        if slow > 0 {
            tokio::time::sleep(Duration::from_millis(slow)).await;
        }
        let mut s = me.s.lock().unwrap();
        s.in_flight -= 1;
        s.ops.push(Op { status: 200, ..op });
        Ok(())
    }

    async fn drop_transfer(Held(me): Held<Scripted>, Part((user, id)): Part<(String, String)>) -> Response {
        let batch = id.rsplit_once('-').map_or(id.clone(), |(b, _)| b.to_string());
        if let Err(r) = small(&me, Kind::DropTransfer, user, batch.clone()).await {
            return r;
        }
        if let Some(b) = me.s.lock().unwrap().batches.get_mut(&batch) {
            b.dropped = true;
        }
        StatusCode::NO_CONTENT.into_response()
    }

    async fn take_search(Held(me): Held<Scripted>, axum::Json(b): axum::Json<serde_json::Value>) -> Response {
        match small(&me, Kind::Search, String::new(), b["searchText"].as_str().unwrap_or_default().to_string()).await {
            Ok(()) => axum::Json(json!({})).into_response(),
            Err(r) => r,
        }
    }

    async fn searched(Held(me): Held<Scripted>, Part(id): Part<String>) -> Response {
        read(&me, format!("searches/{id}")).unwrap_or_else(|| axum::Json(json!({ "isComplete": true })).into_response())
    }

    async fn responses(Held(me): Held<Scripted>, Part(id): Part<String>) -> Response {
        if let Some(r) = read(&me, format!("searches/{id}/responses")) {
            return r;
        }
        {
            let mut s = me.s.lock().unwrap();
            if s.responses_busy > 0 {
                s.responses_busy -= 1;
                return (StatusCode::TOO_MANY_REQUESTS, ONE_AT_A_TIME).into_response();
            }
        }
        axum::Json(serde_json::Value::Array(me.s.lock().unwrap().answers.clone())).into_response()
    }

    async fn forget_search(Held(me): Held<Scripted>, Part(id): Part<String>) -> Response {
        match small(&me, Kind::ForgetSearch, String::new(), id).await {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(r) => r,
        }
    }

    async fn unexpected(Held(me): Held<Scripted>, uri: axum::http::Uri) -> Response {
        me.s.lock().unwrap().violations.push(format!("an unexpected call: {uri}"));
        (StatusCode::IM_A_TEAPOT, "not expected").into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// slskd at a local address: its batches answered `status` and `body`, its own state `state`.
    async fn answering(status: u16, body: serde_json::Value, state: serde_json::Value) -> Slskd {
        let code = axum::http::StatusCode::from_u16(status).unwrap();
        let app = axum::Router::new()
            .route("/api/v0/transfers/downloads/batches", axum::routing::post(move || std::future::ready((code, axum::Json(body.clone())))))
            .route("/api/v0/application", axum::routing::get(move || std::future::ready(axum::Json(state.clone()))));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        crate::run::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Slskd::new(&url, "key").unwrap()
    }

    #[tokio::test]
    async fn a_search_listens_for_fifteen_seconds_which_slskd_counts_in_milliseconds() {
        let sent = std::sync::Arc::new(std::sync::Mutex::new(serde_json::Value::Null));
        let kept = sent.clone();
        let app = axum::Router::new().route(
            "/api/v0/searches",
            axum::routing::post(move |axum::Json(body): axum::Json<serde_json::Value>| {
                *kept.lock().unwrap() = body;
                std::future::ready(axum::Json(serde_json::json!({})))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        crate::run::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Slskd::new(&url, "key").unwrap().search("Air Moon Safari").await.unwrap();
        assert_eq!(sent.lock().unwrap()["searchTimeout"], 15_000);
    }

    fn offered() -> Vec<Offered> {
        vec![Offered { filename: "d\\01.flac".into(), size: 1, extension: None, bit_rate: None, bit_depth: None, sample_rate: None, length: None, is_locked: false }]
    }

    /// Whether a fetch of `slskd`'s failed for slskd's sake, and what it said.
    async fn failure_of(slskd: &Slskd) -> (bool, String) {
        let e = slskd.fetch_many(&new_batch(), "ann", &offered(), || true).await.unwrap_err();
        let single = slskd.fetch("ann", &offered()[0]).await.unwrap_err();
        assert_eq!(single.is::<Away>(), e.is::<Away>(), "one file alike");
        (e.is::<Away>(), format!("{e:#}"))
    }

    #[tokio::test]
    async fn a_fetch_slskd_itself_could_not_make_is_told_from_a_user_who_refused_it() {
        let on = serde_json::json!({ "server": { "isLoggedIn": true } });
        let off = serde_json::json!({ "server": { "isLoggedIn": false } });
        let null = serde_json::Value::Null;
        // The user's: offline, or slskd saying they would not send.
        assert_eq!(failure_of(&answering(404, null.clone(), on.clone()).await).await, (false, "ann is offline".to_string()));
        let refused = serde_json::json!({ "batch": null, "failures": [{ "message": "queue full" }] });
        assert_eq!(slskd_says(&answering(200, refused, on.clone()).await).await, "ann would not send them: queue full");
        // An error, with slskd well: the user's, as the owner has it. Of another shape, what it said stands.
        assert!(!failure_of(&answering(500, null.clone(), on.clone()).await).await.0);
        assert!(!failure_of(&answering(500, null.clone(), serde_json::json!({ "version": "x" })).await).await.0);
        // slskd's: not signed in to Soulseek yet, its key refused, a proxy saying it is not up.
        assert!(failure_of(&answering(500, null.clone(), off).await).await.0);
        for status in [401, 403, 502, 503, 504] {
            assert!(failure_of(&answering(status, null.clone(), on.clone()).await).await.0, "{status}");
        }
        // Changed by round 1.3's plan (SK2.5): a 429 is slskd busy, asked again in its lane: a 429 then a 200 is one batch taken.
        let busy = scripted::Scripted::new().busy(1);
        let batch = new_batch();
        busy.client().fetch_many(&batch, "ann", &offered(), || true).await.expect("taken at the second try");
        assert_eq!((busy.posts().len(), busy.batches_of("ann")), (2, vec![batch]), "one batch taken");
        let single = scripted::Scripted::new().busy(1);
        single.client().fetch("ann", &offered()[0]).await.expect("one file alike");
        assert_eq!(single.batches_of("ann").len(), 1);
        // Nothing listening.
        let gone = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap();
        let (away, said) = failure_of(&Slskd::new(&format!("http://{gone}"), "key").unwrap()).await;
        assert!(away);
        assert!(said.starts_with("slskd could not be reached: "), "{said}");
    }

    /// The review of part 2, L1: a single file's answer lost is read back by its batch's id, as a record's is (SK2.3): slskd has it,
    /// so it is under way; the connection cut before slskd took it, so it is sent again under that id, once. Never slskd away, never
    /// two batches of one file.
    #[tokio::test]
    async fn a_single_files_lost_answer_is_read_back_and_never_fetched_twice() {
        let lost = scripted::Scripted::new().lose_answer_of_first_post();
        let batch = lost.client().fetch("peer-a", &offered()[0]).await.expect("slskd has it: under way");
        assert_eq!(lost.batches_of("peer-a"), [batch.clone()], "one batch");
        assert_eq!(lost.reads_of(&batch), 1, "read back by its id");
        let cut = scripted::Scripted::new().cut_before_taking();
        let batch = cut.client().fetch("peer-a", &offered()[0]).await.expect("sent again under its id");
        let tries: Vec<String> = cut.posts().into_iter().filter(|o| o.status != 429).map(|o| o.batch).collect();
        assert_eq!(tries, [batch.clone(), batch.clone()], "the same id, twice in all");
        assert_eq!(cut.batches_of("peer-a"), [batch]);
        assert!(lost.violations().is_empty() && cut.violations().is_empty(), "{:?} {:?}", lost.violations(), cut.violations());
    }

    async fn slskd_says(slskd: &Slskd) -> String {
        let e = slskd.fetch_many(&new_batch(), "ann", &offered(), || true).await.unwrap_err();
        assert!(!e.is::<Away>());
        format!("{e:#}")
    }

    /// SK2.6 (kind 6, one turn per service): slskd answering one operation 429 "Only one concurrent operation is permitted" holds
    /// that operation alone, a second, then doubling: a read of another batch goes on meanwhile, and nothing is kept for later,
    /// so the next operation goes at once. For 0.18, TN's slskd turn is this lane, and this 429 is no limit answer.
    #[tokio::test]
    async fn a_busy_answer_holds_only_its_own_operation() {
        let slskd = scripted::Scripted::new().busy(1).real_clock();
        let client = slskd.client();
        let files = offered();
        let other = new_batch();
        let ((post, posted), (read, read_at)) = tokio::join!(
            async { (client.fetch_many(&new_batch(), "peer-a", &files, || true).await, std::time::Instant::now()) },
            async { (client.transfers(&other).await, std::time::Instant::now()) }
        );
        post.expect("taken once slskd was free");
        assert!(read.unwrap_err().is::<NotKnown>(), "the read answered: slskd knows no such batch");
        let posts = slskd.posts();
        assert_eq!(posts.iter().map(|p| p.status).collect::<Vec<_>>(), [429, 200], "that operation tried again");
        assert!(posts[1].began_ms >= posts[0].began_ms + 1000, "a second later: {posts:?}");
        assert!(read_at + Duration::from_millis(500) < posted, "the read went on meanwhile, not after the operation's wait");
        assert_eq!(slskd.reads_of(&other), 1);
        // Nothing kept for later: the next operation goes at once, with no wait of a minute.
        let took = std::time::Instant::now();
        client.fetch_many(&new_batch(), "peer-b", &files, || true).await.unwrap();
        assert!(took.elapsed() < Duration::from_millis(900), "{:?}", took.elapsed());
        assert!(slskd.violations().is_empty(), "{:?}", slskd.violations());
    }

    fn file(name: &str, secs: i64, locked: bool) -> Offered {
        Offered { filename: name.into(), size: 1000, extension: None, bit_rate: Some(320), bit_depth: None, sample_rate: None, length: Some(secs), is_locked: locked }
    }

    fn answer(user: &str, slot: bool, files: Vec<Offered>) -> Answer {
        Answer { username: user.into(), files, has_free_upload_slot: slot, queue_length: 0, upload_speed: 100 }
    }

    #[test]
    fn candidates_are_playable_unlocked_of_the_length_and_lossless_first() {
        let answers = vec![
            answer("a", true, vec![file(r"Music\Artist\Album\06 Song.mp3", 241, false), file(r"Music\06 Song.jpg", 0, false)]),
            answer("b", false, vec![file(r"x\06 Song.flac", 240, false), file(r"x\06 Song (edit).flac", 200, false)]),
            answer("c", true, vec![file(r"y\06 Song.flac", 242, true)]),
        ];
        let c = candidates(answers, Some(240_500));
        let names: Vec<(&str, &str)> = c.iter().map(|c| (c.user.as_str(), c.format.as_str())).collect();
        assert_eq!(names, [("b", "flac"), ("a", "mp3")], "locked, not audio and the edit (40 s short) left out");
        assert_eq!(c[0].id, 0);
        assert_eq!(base_name(&c[1].file), "06 Song.mp3");
    }

    #[test]
    fn ids_and_names_are_as_slskd_takes_them() {
        let id = uuid();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
        assert_eq!(segment("DJ Ünter/x"), "DJ%20%C3%9Cnter%2Fx");
    }

    #[test]
    fn a_length_not_known_keeps_the_file() {
        let mut f = file(r"z\Song.flac", 0, false);
        f.length = None;
        assert_eq!(candidates(vec![answer("z", true, vec![f])], Some(240_000)).len(), 1);
    }

    #[test]
    fn a_batchs_folder_is_known_by_its_id() {
        assert!(is_batch(&uuid()));
        assert!(!is_batch("Old drive"));
        assert!(!is_batch("0b6f4bd4-2f0c-4b7e-9a51-6c1c0d8f7a1g"));
    }
}
