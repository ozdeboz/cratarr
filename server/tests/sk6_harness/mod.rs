//! SK6's test equipment (SCENARIOS-SK17-GATE, "What each test needs"): the gate cases as Soulseek's users share them, a slskd sharing
//! them (`slskd_sharing`), Incoming taking Soulseek's records, Soulseek's queue as a restart reads it, and the readers the scenarios
//! name: the item as `soulseek.json` keeps it, its states sampled over time, a record's folder watched as it arrives, and the
//! arrival's card once heard. The SK6 tests themselves are e2e.rs's; bring these in with `use sk6_harness::*;`.

#![allow(dead_code)]

mod slskd_sharing;
#[allow(unused_imports)]
pub use slskd_sharing::*;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::Server;

/// Tide Tables as its users share it (TIDE is where Cratarr places it).
pub const TIDE_DIR: &str = "Glass Harbour - Tide Tables (2012)";
/// Harbour Lights where Cratarr places it, and as its users share it.
pub const LIGHTS: &str = "Glass Harbour/Harbour Lights (2014)";
pub const LIGHTS_DIR: &str = "Glass Harbour - Harbour Lights (2014)";
/// Each record's list: its tracks' titles and lengths (s), as the gate cases make them (`fixtures::gate`'s TIDE_TABLES and
/// HARBOUR_LIGHTS, which this crate cannot name).
pub const TIDE_LIST: [(&str, u32); 3] = [("Slack Water", 5), ("Neap", 4), ("Spring Tide", 7)];
pub const LIGHTS_LIST: [(&str, u32); 3] = [("Pier", 4), ("Breakwater", 5), ("Lantern", 6)];
/// The case folders `--make-gate-cases` makes: full band; walled at 16 kHz; clipping, a click and a pop at the very end
/// (Tide Tables only).
pub const CLEAN: &str = "clean";
pub const WALLED: &str = "walled";
pub const NOTES: &str = "notes";

/// The gate cases, made by the server (`--make-gate-cases`) once a run and copied to the test's own folder.
pub struct Cases {
    pub dir: PathBuf,
}

/// The gate cases at `to`.
pub fn gate_cases(to: &Path) -> Cases {
    static MADE: OnceLock<PathBuf> = OnceLock::new();
    super::made_once(&MADE, "--make-gate-cases", to);
    Cases { dir: to.to_path_buf() }
}

impl Cases {
    /// A case's folder of a record: `case` ("clean", "walled", "notes"), `folder` (`TIDE_DIR`, `LIGHTS_DIR`).
    pub fn folder(&self, case: &str, folder: &str) -> PathBuf {
        self.dir.join(case).join(folder)
    }

    /// `user` sharing that case's folder as `Music\<folder>\…` (its `dir` the case's folder: what they send, byte for byte).
    pub fn share(&self, user: &str, case: &str, folder: &str) -> Share {
        let dir = self.folder(case, folder);
        assert!(dir.is_dir(), "no case {case}/{folder}");
        Share::new(user, &dir, folder)
    }
}

/// Settings as 0.16 writes them: Music the main library, Incoming taking Soulseek's records (its fetcher "soulseek"), and the
/// own-music folder.
pub fn soulseek_settings(data: &Path, music: &Path, incoming: &Path, own_music: &Path) {
    let mut source = super::own_source(&[(music, "main"), (incoming, "incoming")], own_music);
    source["libraries"][1]["fetcher"] = json!("soulseek");
    let settings = json!({ "version": 4, "music_folder": music.to_string_lossy(), "sources": [source] });
    std::fs::write(data.join("settings.json"), serde_json::to_vec_pretty(&settings).unwrap()).unwrap();
}

/// A Want's item as `soulseek.json` keeps it, waiting and due now: its record (the request's), the request, its artist, album and
/// `year`, and the record's list in `tracks`, each track's length (`list`: title, seconds), no recordings (Discogs' shape).
/// `request` is the request as `/api/requests/want` answers it.
pub fn waiting(id: &str, request: &Value, year: i32, list: &[(&str, u32)]) -> Value {
    let tracks: Vec<Value> = list.iter().zip(1..).map(|((title, secs), n)| json!({ "disc": 1, "position": n, "title": title, "length_ms": secs * 1000 })).collect();
    let record = json!({ "record": request["record"].as_i64().unwrap_or(0), "artist": request["artist"], "album": request["album"], "year": year, "request": request["id"] });
    json!({ "id": id, "record": record, "state": "waiting", "due": 0, "tracks": tracks })
}

/// Soulseek's queue as the next start reads it (`soulseek.json` in `data`): these items, and the next new one numbered after them.
pub fn write_queue(data: &Path, items: &[Value]) {
    let next = items.iter().filter_map(|i| i["id"].as_str()?.strip_prefix("ss-")?.parse::<u64>().ok()).max().unwrap_or(0).max(items.len() as u64);
    std::fs::write(data.join("soulseek.json"), serde_json::to_vec_pretty(&json!({ "next": next, "items": items })).unwrap()).unwrap();
}

/// Now, in seconds since the epoch, as an item's times are kept.
pub fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64
}

/// The item `id` as `soulseek.json` in `data` keeps it now; Null where it is not there. The server writes the file whole (a
/// rename), so a read that cannot be parsed is the file being replaced: read again.
pub fn kept_item(data: &Path, id: &str) -> Value {
    let p = data.join("soulseek.json");
    for _ in 0..50 {
        let Ok(bytes) = std::fs::read(&p) else { return Value::Null };
        if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
            return v["items"].as_array().into_iter().flatten().find(|i| i["id"] == id).cloned().unwrap_or(Value::Null);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("{} could not be read", p.display())
}

/// `/api/soulseek` read in the background every 250 ms, as the server's acting account at the call (`act_as`): each item as it was
/// seen, in turn. Stops when dropped.
pub struct Sampled {
    seen: Arc<Mutex<BTreeMap<String, Vec<Value>>>>,
    task: tokio::task::JoinHandle<()>,
}

/// Starts sampling `s`'s Soulseek list. In the test's runtime.
pub fn sampled(s: &Server) -> Sampled {
    let (http, url, cookie) = (s.http.clone(), format!("{}/api/soulseek", s.base), s.acting.lock().unwrap().clone());
    let seen: Arc<Mutex<BTreeMap<String, Vec<Value>>>> = Arc::default();
    let into = seen.clone();
    let task = tokio::spawn(async move {
        loop {
            let mut r = http.get(&url);
            if let Some(c) = &cookie {
                r = r.header(reqwest::header::COOKIE, c);
            }
            if let Ok(answer) = r.send().await
                && let Ok(v) = answer.json::<Value>().await
            {
                let mut seen = into.lock().unwrap();
                for i in v["items"].as_array().into_iter().flatten() {
                    let Some(id) = i["id"].as_str() else { continue };
                    let was = seen.entry(id.to_string()).or_default();
                    if was.last() != Some(i) {
                        was.push(i.clone());
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    });
    Sampled { seen, task }
}

impl Sampled {
    /// Item `id` as each sample saw it, in turn: a sample like the one before left out.
    pub fn items(&self, id: &str) -> Vec<Value> {
        self.seen.lock().unwrap().get(id).cloned().unwrap_or_default()
    }

    /// The states item `id` went through, in turn, a state once for each stretch it held (one gone back to is listed again).
    pub fn states(&self, id: &str) -> Vec<String> {
        self.in_turn(id, "state")
    }

    /// The whys item `id` went through, in turn, a why once for each stretch it held; none left out.
    pub fn whys(&self, id: &str) -> Vec<String> {
        self.in_turn(id, "why")
    }

    fn in_turn(&self, id: &str, field: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for i in self.items(id) {
            if let Some(v) = i[field].as_str()
                && out.last().map(String::as_str) != Some(v)
            {
                out.push(v.to_string());
            }
        }
        out
    }
}

impl Drop for Sampled {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// A record's folder watched as it arrives (Y10, "through the hidden folder"): polled every 20 ms in a thread of its own.
pub struct Watched {
    fewest: Arc<Mutex<Option<usize>>>,
    stop: Arc<AtomicBool>,
    dir: PathBuf,
}

/// Watches `dir`: the fewest audio files it held whenever it was there.
pub fn whole_when_seen(dir: &Path) -> Watched {
    let (fewest, stop) = (Arc::new(Mutex::new(None::<usize>)), Arc::new(AtomicBool::new(false)));
    let (into, stopped, watched) = (fewest.clone(), stop.clone(), dir.to_path_buf());
    let dir = watched.clone();
    std::thread::spawn(move || {
        while !stopped.load(Ordering::SeqCst) {
            if dir.is_dir() {
                let n = audio_in(&dir);
                let mut f = into.lock().unwrap();
                *f = Some(f.map_or(n, |m| m.min(n)));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    });
    Watched { fewest, stop, dir: watched }
}

impl Watched {
    /// The fewest audio files the folder held whenever it was seen; none where it never was. It takes a last look of its own first
    /// (lane C's proof, 2026-11-21: a placing can end between two of the thread's looks, and a loaded machine can hold the thread
    /// past it, so a folder placed whole went unseen): a folder there now is always seen, and what an earlier look saw still counts.
    pub fn get(&self) -> Option<usize> {
        let mut f = self.fewest.lock().unwrap();
        if self.dir.is_dir() {
            let n = audio_in(&self.dir);
            *f = Some(f.map_or(n, |m| m.min(n)));
        }
        *f
    }
}

impl Drop for Watched {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// The audio files under `dir`, a disc's folder's too; what cannot be read counts none (it may be going as it is read).
fn audio_in(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| {
            let p = e.path();
            if p.is_dir() {
                audio_in(&p)
            } else {
                usize::from(p.extension().is_some_and(|x| x == "flac" || x == "mp3"))
            }
        })
        .sum()
}

/// The hidden folders a placing makes (`.cratarr-<record>-<request>.part`) left in `library`, by name.
pub fn hidden_left(library: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(library)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(".cratarr-") && n.ends_with(".part"))
        .collect();
    out.sort();
    out
}

/// The intake card of `album` (as Incoming lists it) in `library`, once its sound is heard (as
/// `the_card_hears_the_incoming_copy_end_to_end` waits for it), read as the server's acting account. Its hearing begun as an
/// administrator's open of the arrival begins it (LOOKUPS LK.B1: the card's GET hears nothing).
pub async fn heard_card(s: &Server, album: &Value, library: &Path) -> Value {
    let path = format!("/api/albums/cratarr/{}/card?from={}", album["id"].as_str().unwrap(), library.to_string_lossy().replace('/', "%2F"));
    s.ok(reqwest::Method::POST, "/api/records/open", serde_json::json!({ "record": album["id"], "from": "incoming" })).await;
    let start = Instant::now();
    loop {
        let card = s.get(&path).await;
        if card["checks"].as_array().is_some_and(|c| c.iter().any(|c| c["key"] == "sound" && c["state"] != "waiting")) {
            return card;
        }
        assert!(start.elapsed() < Duration::from_secs(120), "waited for the card's sound: {card}");
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// A card's sound check: (its state, what it says).
pub fn sound(card: &Value) -> (&str, &str) {
    let c = card["checks"].as_array().and_then(|c| c.iter().find(|c| c["key"] == "sound")).unwrap_or_else(|| panic!("no sound check: {card}"));
    (c["state"].as_str().unwrap_or_default(), c["said"].as_str().unwrap_or_default())
}
