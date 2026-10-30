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
/// How long slskd listens for answers (its own default).
const LISTEN_SECS: u64 = 15;

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
        let r = self.http.get(self.at(path)).header("X-API-Key", &self.key).send().await.context("slskd could not be reached")?;
        let status = r.status();
        if !status.is_success() {
            bail!("slskd answered {status}");
        }
        r.json().await.context("slskd's answer could not be read")
    }

    /// Its version, which also says it answers and the key works.
    pub async fn version(&self) -> Result<String> {
        self.get("application/version").await
    }

    /// Starts a search; its id.
    pub async fn search(&self, text: &str) -> Result<String> {
        let id = uuid();
        let body = json!({ "id": id, "searchText": text, "searchTimeout": LISTEN_SECS, "filterResponses": true, "minimumResponseFileCount": 1 });
        let r = self.http.post(self.at("searches")).header("X-API-Key", &self.key).json(&body).send().await.context("slskd could not be reached")?;
        let status = r.status();
        if !status.is_success() {
            bail!("slskd would not search: {status} {}", r.text().await.unwrap_or_default());
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
        let r = self.http.post(self.at("transfers/downloads/batches")).header("X-API-Key", &self.key).json(&body).send().await.context("slskd could not be reached")?;
        let status = r.status();
        if status.as_u16() == 404 {
            bail!("{user} is offline");
        }
        if !status.is_success() {
            bail!("slskd would not fetch it: {status} {}", r.text().await.unwrap_or_default());
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
