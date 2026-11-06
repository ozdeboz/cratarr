//! slskd's REST API (0.9.8, single-track replacement): one search for one track, its answers, and one file fetched.
//! Never bulk (the March 2026 lesson: unattended Soulseek batches put wrong files into the library); the admin picks
//! the file, and the gate (`acoustid`) decides whether it may go in.
//!
//! A fetch goes into its own folder under slskd's completed downloads (a batch's destination, slskd 0.26 or later:
//! `cratarr/<batch>`), so nothing else that watches that folder takes it for its own; Cratarr moves it out to its
//! staging the moment it completes.

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

/// Whether an answer is slskd's own trouble, whoever was asked for: its key refused, its limit, or a proxy saying it is
/// not up.
fn its_own(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 401 | 403 | 429 | 502 | 503 | 504)
}

/// slskd at an address, with its API key.
#[derive(Clone)]
pub struct Slskd {
    url: String,
    key: String,
    http: reqwest::Client,
}

/// One file offered in answer to a search.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
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
        let http = reqwest::Client::builder().timeout(Duration::from_secs(20)).build()?;
        Ok(Self { url: url.trim_end_matches('/').to_string(), key: key.to_string(), http })
    }

    fn at(&self, path: &str) -> String {
        format!("{}/api/v0/{path}", self.url)
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let r = self.http.get(self.at(path)).header("X-API-Key", &self.key).send().await.map_err(unreached)?;
        let status = r.status();
        if its_own(status) {
            return Err(Away(format!("slskd answered {status}")).into());
        }
        if !status.is_success() {
            bail!("slskd answered {status}");
        }
        r.json().await.context("slskd's answer could not be read")
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
        let r = self.http.post(self.at("searches")).header("X-API-Key", &self.key).json(&body).send().await.map_err(unreached)?;
        let status = r.status();
        if !status.is_success() {
            let said = format!("slskd would not search: {status} {}", r.text().await.unwrap_or_default());
            return Err(self.turned_down(status, said).await);
        }
        Ok(id)
    }

    /// Whether the search is over; its answers are read only after (they are not all there before).
    pub async fn searched(&self, id: &str) -> Result<bool> {
        Ok(self.get::<Search>(&format!("searches/{id}")).await?.is_complete)
    }

    pub async fn answers(&self, id: &str) -> Result<Vec<Answer>> {
        self.get(&format!("searches/{id}/responses")).await
    }

    /// What slskd saying no comes to. A 500 is how it says both "that user failed" and "I am not on Soulseek": there it is
    /// asked which, as a slskd just back and not signed in yet would otherwise fail every user in a moment.
    async fn turned_down(&self, status: reqwest::StatusCode, said: String) -> anyhow::Error {
        if its_own(status) || self.off_soulseek().await { Away(said).into() } else { anyhow::anyhow!(said) }
    }

    /// Whether slskd says it is not signed in to Soulseek, or does not answer. Its word as far as it goes: any other shape
    /// of answer says nothing, and what it said stands.
    async fn off_soulseek(&self) -> bool {
        match self.get::<serde_json::Value>("application").await {
            Ok(v) => v["server"]["isLoggedIn"] == false,
            Err(e) => e.is::<Away>(),
        }
    }

    /// The search gone from slskd's list.
    pub async fn forget(&self, id: &str) {
        let _ = self.http.delete(self.at(&format!("searches/{id}"))).header("X-API-Key", &self.key).send().await;
    }

    /// Asks `user` for one file, into its own folder (`cratarr/<batch>`); the batch's id, which names that folder.
    pub async fn fetch(&self, user: &str, file: &Offered) -> Result<String> {
        let batch = uuid();
        let body = json!({
            "id": batch,
            "username": user,
            "files": [{ "filename": file.filename, "size": file.size }],
            "options": { "destination": format!("{FOLDER}/{batch}") },
        });
        let r = self.http.post(self.at("transfers/downloads/batches")).header("X-API-Key", &self.key).json(&body).send().await.map_err(unreached)?;
        let status = r.status();
        if status.as_u16() == 404 {
            bail!("{user} is offline");
        }
        if !status.is_success() {
            let said = format!("slskd would not fetch it: {status} {}", r.text().await.unwrap_or_default());
            return Err(self.turned_down(status, said).await);
        }
        let e: Enqueued = r.json().await.context("slskd's answer could not be read")?;
        if let Some(f) = e.failures.first() {
            bail!("slskd would not fetch it: {}", f.message);
        }
        if e.batch.is_none() {
            bail!("slskd did not take the fetch");
        }
        Ok(batch)
    }

    /// Asks `user` for a whole folder's files in one batch, into its own folder (`cratarr/<batch>`): a record fetched
    /// whole (0.15, Soulseek in Cratarr). The batch's id.
    pub async fn fetch_many(&self, user: &str, files: &[Offered]) -> Result<String> {
        let batch = uuid();
        let listed: Vec<serde_json::Value> = files.iter().map(|f| json!({ "filename": f.filename, "size": f.size })).collect();
        let body = json!({ "id": batch, "username": user, "files": listed, "options": { "destination": format!("{FOLDER}/{batch}") } });
        let r = self.http.post(self.at("transfers/downloads/batches")).header("X-API-Key", &self.key).json(&body).send().await.map_err(unreached)?;
        let status = r.status();
        if status.as_u16() == 404 {
            bail!("{user} is offline");
        }
        if !status.is_success() {
            let said = format!("slskd would not fetch them: {status} {}", r.text().await.unwrap_or_default());
            return Err(self.turned_down(status, said).await);
        }
        let e: Enqueued = r.json().await.context("slskd's answer could not be read")?;
        if let Some(f) = e.failures.first() {
            bail!("{user} would not send them: {}", f.message);
        }
        if e.batch.is_none() {
            bail!("slskd did not take the fetch");
        }
        Ok(batch)
    }

    /// A batch's transfers, as they are now.
    pub async fn transfers(&self, batch: &str) -> Result<Vec<Transfer>> {
        let b: Batch = self.get(&format!("transfers/downloads/batches/{batch}")).await?;
        Ok(b.transfers.unwrap_or_default())
    }

    /// The fetch's transfer, as it is now.
    pub async fn transfer(&self, batch: &str) -> Result<Transfer> {
        let b: Batch = self.get(&format!("transfers/downloads/batches/{batch}")).await?;
        b.transfers.and_then(|t| t.into_iter().next()).context("slskd has no transfer for it")
    }

    /// The transfer gone from slskd's list (and stopped, when it still runs).
    pub async fn drop_transfer(&self, user: &str, id: &str) {
        let path = format!("transfers/downloads/{}/{id}?remove=true", segment(user));
        let _ = self.http.delete(self.at(&path)).header("X-API-Key", &self.key).send().await;
    }
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
#[derive(Debug, Clone, Serialize, PartialEq)]
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
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
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
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Slskd::new(&url, "key").unwrap().search("Air Moon Safari").await.unwrap();
        assert_eq!(sent.lock().unwrap()["searchTimeout"], 15_000);
    }

    fn offered() -> Vec<Offered> {
        vec![Offered { filename: "d\\01.flac".into(), size: 1, extension: None, bit_rate: None, bit_depth: None, sample_rate: None, length: None, is_locked: false }]
    }

    /// Whether a fetch of `slskd`'s failed for slskd's sake, and what it said.
    async fn failure_of(slskd: &Slskd) -> (bool, String) {
        let e = slskd.fetch_many("ann", &offered()).await.unwrap_err();
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
        for status in [401, 403, 429, 502, 503, 504] {
            assert!(failure_of(&answering(status, null.clone(), on.clone()).await).await.0, "{status}");
        }
        // Nothing listening.
        let gone = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap();
        let (away, said) = failure_of(&Slskd::new(&format!("http://{gone}"), "key").unwrap()).await;
        assert!(away);
        assert!(said.starts_with("slskd could not be reached: "), "{said}");
    }

    async fn slskd_says(slskd: &Slskd) -> String {
        let e = slskd.fetch_many("ann", &offered()).await.unwrap_err();
        assert!(!e.is::<Away>());
        format!("{e:#}")
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
