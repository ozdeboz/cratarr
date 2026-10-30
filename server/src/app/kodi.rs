//! Play on Kodi (the owner, 2026-10-16): a Kodi as one of Play On's players, driven by Cratarr itself over Kodi's
//! JSON-RPC (HTTP, "Allow remote control via HTTP"). Nothing is installed on Kodi: Cratarr fills Kodi's music playlist
//! with its tracks' addresses (`/api/file/...`, which Kodi streams and seeks in), and plays, pauses, skips and seeks it.
//!
//! The driver joins Play On as the app does (`PlayOn::connect_kodi`): Cratarr's commands come to it as to any player,
//! and it reports what Kodi plays, read every second while it plays. A Kodi out of reach is offline; the driver tries
//! again every few seconds. What Kodi does on its own (a film) is not Cratarr's: it reports nothing playing then.
//!
//! In a group (decision 0037, amended: the owner), a Kodi follows and never leads: it plays the group's sound as one
//! long WAV from Cratarr (`snapcast::listen`), silenced at once at a break as the others are, and kept in line from
//! its own time in the stream: Cratarr skips or pads a few milliseconds of what it sends.
//!
//! A long queue (a playlist of hundreds) takes Kodi a while to take in (it looks up every address): Kodi's playlist
//! holds the queue's first tracks, as many as the playing one needs, at once; the rest follows a chunk at a time
//! between the reads.
//!
//! Not on Kodi: Radio and Autoplay (the app's player inserts them).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Weak;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_stream::StreamExt;

use super::playon::{Command, PlayOn, QueueTrack, Report, Sleep};
use super::queue::Repeat;

/// A Kodi to play on, as kept in `players.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct KodiConfig {
    /// Its name among the players ("Bedroom").
    pub name: String,
    /// Kodi's web server, e.g. `http://libreelec.local:8080`.
    pub url: String,
    #[serde(default)]
    pub user: String,
    /// Kept in `players.json` only; never shown (blank keeps it when saved).
    #[serde(default)]
    pub password: String,
    /// Cratarr's address as Kodi reaches it, e.g. `http://192.168.1.5:8081`: the tracks' addresses start with it.
    #[serde(default)]
    pub stream_from: String,
    /// The administrator who added it, whose device it is (decision 0051); none: from before, the first one's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

/// Kodi's music player and playlist.
const PLAYER: u64 = 0;
const PLAYLIST: u64 = 0;
/// How often Kodi is read while it plays, and while it does not.
const POLL_PLAYING: Duration = Duration::from_secs(1);
const POLL_IDLE: Duration = Duration::from_secs(4);
/// Out of reach this many reads in a row: offline.
const MISSES: u32 = 3;
/// A play waits for Kodi's playlist to hold its first track and this many after it; the rest is added this many at a
/// time (a single Playlist.Add of 472 took a Pi 3 longer than the usual 6 s), each allowed this long.
const AHEAD: usize = 3;
const CHUNK: usize = 50;
const FILL_TIMEOUT: Duration = Duration::from_secs(30);

/// Kodi's JSON-RPC.
struct Rpc {
    http: reqwest::Client,
    endpoint: String,
    user: String,
    password: String,
    id: AtomicU64,
}

impl Rpc {
    fn new(cfg: &KodiConfig) -> Self {
        let base = cfg.url.trim().trim_end_matches('/').trim_end_matches("/jsonrpc").to_string();
        Self {
            http: reqwest::Client::builder().timeout(Duration::from_secs(6)).build().expect("a plain client builds"),
            endpoint: format!("{base}/jsonrpc"),
            user: cfg.user.clone(),
            password: cfg.password.clone(),
            id: AtomicU64::new(1),
        }
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value> {
        self.call_for(method, params, None).await
    }

    /// A call allowed longer than the usual 6 s.
    async fn call_for(&self, method: &str, params: Value, timeout: Option<Duration>) -> Result<Value> {
        let id = self.id.fetch_add(1, Ordering::Relaxed);
        let mut req = self.http.post(&self.endpoint).json(&json!({ "jsonrpc": "2.0", "method": method, "params": params, "id": id }));
        if let Some(t) = timeout {
            req = req.timeout(t);
        }
        if !self.user.is_empty() {
            req = req.basic_auth(&self.user, Some(&self.password));
        }
        let resp = req.send().await.with_context(|| format!("Kodi at {}", self.endpoint))?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(anyhow!("Kodi refused the user name or password"));
        }
        let body: Value = resp.error_for_status()?.json().await?;
        if let Some(e) = body.get("error") {
            return Err(anyhow!("Kodi: {method}: {}", e.get("message").and_then(Value::as_str).unwrap_or("error")));
        }
        Ok(body.get("result").cloned().unwrap_or(Value::Null))
    }
}

/// A track's address for Kodi.
fn address(stream_from: &str, t: &QueueTrack) -> String {
    match reqwest::Url::parse(stream_from.trim()) {
        Ok(mut u) => {
            if let Ok(mut path) = u.path_segments_mut() {
                path.pop_if_empty().extend(["api", "file", t.source.as_str(), t.id.as_str()]);
            }
            u.to_string()
        }
        Err(_) => format!("{}/api/file/{}/{}", stream_from.trim_end_matches('/'), t.source, t.id),
    }
}

/// A time as Kodi takes it.
fn kodi_time(ms: u64) -> Value {
    json!({ "hours": ms / 3_600_000, "minutes": ms / 60_000 % 60, "seconds": ms / 1000 % 60, "milliseconds": ms % 1000 })
}

/// Milliseconds from Kodi's time.
fn ms_of(t: &Value) -> u64 {
    let n = |k: &str| t.get(k).and_then(Value::as_u64).unwrap_or(0);
    ((n("hours") * 60 + n("minutes")) * 60 + n("seconds")) * 1000 + n("milliseconds")
}

/// What Kodi plays now, as read.
#[derive(Debug, Default, Clone, PartialEq)]
struct Now {
    /// Kodi's music player is on, with one of Cratarr's tracks of the queue at `index`.
    ours: bool,
    index: i64,
    playing: bool,
    position_ms: u64,
    duration_ms: Option<u64>,
    volume: u8,
    repeat: Repeat,
}

/// One Kodi, driven.
struct Driver {
    name: String,
    rpc: Rpc,
    stream_from: String,
    /// The queue as Cratarr told it; Kodi's playlist holds its first `held` tracks (all, once filled).
    queue: Vec<QueueTrack>,
    held: usize,
    /// When the next chunk is added (later after a failure).
    fill_at: tokio::time::Instant,
    /// The queue's version in the last command (reports say it).
    version: Option<u64>,
    /// The queue changed since a report took it.
    queue_changed: bool,
    /// The volume before a duck.
    unducked: Option<u8>,
    sleep: Option<Sleep>,
    /// The track the end-of-track timer waits for (its place when set).
    sleep_place: Option<i64>,
    now: Now,
    /// The group it follows, by its leader; `None` alone.
    following: Option<String>,
    /// When it last opened its group's sound (opened again when Kodi stopped it).
    opened: Option<std::time::Instant>,
}

impl Driver {
    fn new(cfg: &KodiConfig, queue: Vec<QueueTrack>) -> Self {
        Self {
            name: cfg.name.clone(),
            rpc: Rpc::new(cfg),
            stream_from: cfg.stream_from.clone(),
            held: queue.len(),
            queue,
            fill_at: tokio::time::Instant::now(),
            version: None,
            queue_changed: false,
            unducked: None,
            sleep: None,
            sleep_place: None,
            now: Now { index: -1, ..Now::default() },
            following: None,
            opened: None,
        }
    }

    /// Its group's sound, for Kodi to open.
    fn listen_address(&self, leader: &str) -> String {
        let mut u = reqwest::Url::parse(self.stream_from.trim()).unwrap_or_else(|_| reqwest::Url::parse("http://localhost").expect("a plain address"));
        if let Ok(mut path) = u.path_segments_mut() {
            path.pop_if_empty().extend(["api", "groups", leader, "listen.wav"]);
        }
        u.query_pairs_mut().append_pair("player", &self.name);
        u.to_string()
    }

    /// Follows `leader`'s group: its own music stops (a follower keeps no queue), the group's sound plays.
    async fn follow(&mut self, leader: &str) -> Result<()> {
        let _ = self.rpc.call("Player.Stop", json!({ "playerid": PLAYER })).await;
        self.rpc.call("Playlist.Clear", json!({ "playlistid": PLAYLIST })).await?;
        self.queue.clear();
        self.held = 0;
        self.queue_changed = true;
        self.following = Some(leader.to_string());
        self.open_group().await
    }

    async fn open_group(&mut self) -> Result<()> {
        let Some(leader) = self.following.clone() else { return Ok(()) };
        self.opened = Some(std::time::Instant::now());
        // Signed: Kodi sends no sign-in (decision 0043).
        let file = crate::serve::access::signed(&self.listen_address(&leader));
        self.rpc.call("Player.Open", json!({ "item": { "file": file } })).await.map(|_| ())
    }

    /// Following: how far out of line Kodi plays (from its time in the group's sound), and the skip or pad it needs.
    async fn keep_in_line(&mut self) {
        let (Some(snap), Some(_)) = (crate::snapcast::Snapcast::instance(), self.following.as_ref()) else { return };
        let Some(listener) = snap.listener(&self.name) else { return };
        let before = snap.now();
        let Ok(p) = self.rpc.call("Player.GetProperties", json!({ "playerid": PLAYER, "properties": ["time", "speed"] })).await else { return };
        let after = snap.now();
        if p.get("speed").and_then(Value::as_f64).unwrap_or(0.0) <= 0.0 {
            return;
        }
        // What Kodi plays at the read's middle.
        let Some(ms) = p.get("time").map(ms_of) else { return };
        if let Some((ms, late)) = listener.heard(ms as f64, (before + after) / 2, snap.buffer_ms()) {
            tracing::info!(target: "syncplay", player = self.name.as_str(), ms, late, "put back in step");
        }
        if let Some(d) = listener.drift() {
            tracing::info!(target: "syncplay", player = self.name.as_str(), median_ms = d.median_ms, min_ms = d.min_ms, max_ms = d.max_ms, readings = d.readings, corrections = d.corrections, "drift");
        }
    }
    fn place_of(&self, entry: Option<&str>, place: Option<usize>) -> Option<usize> {
        entry.and_then(|e| self.queue.iter().position(|t| t.entry == e)).or(place).filter(|p| *p < self.queue.len())
    }

    fn items(&self, tracks: &[QueueTrack]) -> Value {
        Value::Array(tracks.iter().map(|t| json!({ "file": crate::serve::access::signed(&address(&self.stream_from, t)) })).collect())
    }

    /// Whether Kodi's playlist still lacks some of the queue.
    fn filling(&self) -> bool {
        self.held < self.queue.len()
    }

    /// The next chunk of the queue onto Kodi's playlist, no further than its first `n` tracks.
    async fn fill_step(&mut self, n: usize) -> Result<()> {
        let to = n.min(self.queue.len()).min(self.held + CHUNK);
        if to <= self.held {
            return Ok(());
        }
        let items = self.items(&self.queue[self.held..to]);
        self.rpc.call_for("Playlist.Add", json!({ "playlistid": PLAYLIST, "item": items }), Some(FILL_TIMEOUT)).await?;
        self.held = to;
        Ok(())
    }

    /// Kodi's playlist holding at least the queue's first `n` tracks (all, if fewer): what a command needs there.
    async fn fill_to(&mut self, n: usize) -> Result<()> {
        while self.held < n.min(self.queue.len()) {
            self.fill_step(n).await?;
        }
        Ok(())
    }

    async fn pause(&self, play: Value) -> Result<()> {
        self.rpc.call("Player.PlayPause", json!({ "playerid": PLAYER, "play": play })).await.map(|_| ())
    }

    /// Cratarr's command, on Kodi.
    async fn run(&mut self, c: Command) -> Result<()> {
        match c {
            Command::Play { tracks, index, position_ms, paused, .. } => {
                self.rpc.call("Playlist.Clear", json!({ "playlistid": PLAYLIST })).await?;
                self.queue = tracks.unwrap_or_default();
                self.held = 0;
                self.queue_changed = true;
                if !self.queue.is_empty() {
                    let at = index.min(self.queue.len() - 1);
                    // It plays once its track and a few after it are in; the rest follows between the reads.
                    self.fill_to(at + 1 + AHEAD).await?;
                    self.rpc.call("Player.Open", json!({ "item": { "playlistid": PLAYLIST, "position": at } })).await?;
                    if position_ms > 1000 {
                        // Kodi takes a seek once the track has started.
                        for _ in 0..10 {
                            tokio::time::sleep(Duration::from_millis(300)).await;
                            if self.seek(position_ms).await.is_ok() {
                                break;
                            }
                        }
                    }
                    if paused {
                        self.pause(json!(false)).await?;
                    }
                }
            }
            Command::Add { tracks, at, after, .. } => {
                let place = match after.as_deref() {
                    Some("") => Some(0),
                    Some(e) => self.queue.iter().position(|t| t.entry == e).map(|p| p + 1),
                    None => None,
                }
                .or(at)
                .unwrap_or(self.queue.len())
                .min(self.queue.len());
                // Into what Kodi holds; at or past its end, the fill adds them in their turn.
                if place < self.held {
                    for (i, t) in tracks.iter().enumerate() {
                        let item = json!({ "file": crate::serve::access::signed(&address(&self.stream_from, t)) });
                        self.rpc.call("Playlist.Insert", json!({ "playlistid": PLAYLIST, "position": place + i, "item": item })).await?;
                        self.held += 1;
                    }
                }
                self.queue.splice(place..place, tracks);
                self.queue_changed = true;
            }
            Command::Remove { index, entry } => {
                if let Some(p) = self.place_of(entry.as_deref(), index) {
                    if p < self.held {
                        // Kodi keeps the playing track: past it first.
                        if self.now.ours && self.now.index == p as i64 {
                            self.fill_to(p + 2).await?;
                            let _ = self.rpc.call("Player.GoTo", json!({ "playerid": PLAYER, "to": "next" })).await;
                        }
                        self.rpc.call("Playlist.Remove", json!({ "playlistid": PLAYLIST, "position": p })).await?;
                        self.held -= 1;
                    }
                    self.queue.remove(p);
                    self.queue_changed = true;
                }
            }
            Command::Move { from, to, entry, after } => {
                let Some(from) = self.place_of(entry.as_deref(), from) else { return Ok(()) };
                let to = match after.as_deref() {
                    Some("") => Some(0),
                    Some(e) => self.queue.iter().position(|t| t.entry == e).map(|a| if a > from { a } else { a + 1 }),
                    None => to,
                };
                let Some(to) = to.filter(|t| *t < self.queue.len() && *t != from) else { return Ok(()) };
                self.fill_to(from.max(to) + 1).await?;
                // One place at a time (Kodi swaps; it does not move).
                let (mut at, step): (usize, isize) = (from, if to > from { 1 } else { -1 });
                while at != to {
                    let next = (at as isize + step) as usize;
                    self.rpc.call("Playlist.Swap", json!({ "playlistid": PLAYLIST, "position1": at, "position2": next })).await?;
                    self.queue.swap(at, next);
                    at = next;
                }
                self.queue_changed = true;
            }
            Command::Upcoming { tracks, .. } => {
                let keep = if self.now.ours { (self.now.index + 1).max(0) as usize } else { 0 }.min(self.queue.len());
                for p in (keep..self.held).rev() {
                    self.rpc.call("Playlist.Remove", json!({ "playlistid": PLAYLIST, "position": p })).await?;
                    self.held = p;
                }
                // The new ones follow between the reads.
                self.queue.truncate(keep);
                self.queue.extend(tracks);
                self.queue_changed = true;
            }
            Command::Jump { index, entry } => {
                if let Some(p) = self.place_of(entry.as_deref(), index) {
                    self.fill_to(p + 1 + AHEAD).await?;
                    if self.now.ours {
                        self.rpc.call("Player.GoTo", json!({ "playerid": PLAYER, "to": p })).await?;
                        self.pause(json!(true)).await?;
                    } else {
                        self.rpc.call("Player.Open", json!({ "item": { "playlistid": PLAYLIST, "position": p } })).await?;
                    }
                }
            }
            Command::Pause => self.pause(json!(false)).await?,
            Command::Resume => {
                if self.now.ours {
                    self.pause(json!(true)).await?;
                } else if !self.queue.is_empty() {
                    // Stopped, or Kodi played something else meanwhile: the queue again, where it was.
                    let at = usize::try_from(self.now.index).unwrap_or(0).min(self.queue.len() - 1);
                    self.fill_to(at + 1 + AHEAD).await?;
                    self.rpc.call("Player.Open", json!({ "item": { "playlistid": PLAYLIST, "position": at } })).await?;
                }
            }
            Command::Toggle => {
                if self.now.ours {
                    self.pause(json!("toggle")).await?;
                } else {
                    return Box::pin(self.run(Command::Resume)).await;
                }
            }
            Command::Stop { .. } => {
                let _ = self.rpc.call("Player.Stop", json!({ "playerid": PLAYER })).await;
                self.rpc.call("Playlist.Clear", json!({ "playlistid": PLAYLIST })).await?;
                self.queue.clear();
                self.held = 0;
                self.queue_changed = true;
            }
            Command::Next => {
                if self.now.ours {
                    self.fill_to(usize::try_from(self.now.index + 2).unwrap_or(0)).await?;
                }
                self.rpc.call("Player.GoTo", json!({ "playerid": PLAYER, "to": "next" })).await?;
            }
            Command::Previous => {
                // As a player does: back to the start after a few seconds in, else the track before.
                if self.now.position_ms > 5_000 {
                    self.seek(0).await?;
                } else {
                    self.rpc.call("Player.GoTo", json!({ "playerid": PLAYER, "to": "previous" })).await?;
                }
            }
            Command::Seek { position_ms: Some(ms), .. } => self.seek(ms).await?,
            Command::Volume { level } => {
                let level = level.min(100);
                if self.unducked.is_some() {
                    self.unducked = Some(level);
                } else {
                    self.rpc.call("Application.SetVolume", json!({ "volume": level })).await?;
                }
            }
            Command::Duck => {
                if self.unducked.is_none() {
                    self.unducked = Some(self.now.volume);
                    let quiet = (f32::from(self.now.volume) * 0.25) as u8;
                    self.rpc.call("Application.SetVolume", json!({ "volume": quiet })).await?;
                }
            }
            Command::Unduck => {
                if let Some(v) = self.unducked.take() {
                    self.rpc.call("Application.SetVolume", json!({ "volume": v })).await?;
                }
            }
            Command::Repeat { mode } => {
                let repeat = match mode {
                    Repeat::Off => "off",
                    Repeat::One => "one",
                    Repeat::All => "all",
                };
                if self.now.ours {
                    self.rpc.call("Player.SetRepeat", json!({ "playerid": PLAYER, "repeat": repeat })).await?;
                }
                self.now.repeat = mode;
            }
            // A group (decision 0037, amended): a Kodi follows, never leads.
            Command::Group { leader, .. } => match leader {
                Some(l) if l != self.name => {
                    if self.following.as_deref() != Some(l.as_str()) {
                        self.follow(&l).await?;
                    }
                }
                _ => {
                    if self.following.take().is_some() {
                        let _ = self.rpc.call("Player.Stop", json!({ "playerid": PLAYER })).await;
                        let _ = self.rpc.call("Application.SetMute", json!({ "mute": false })).await;
                    }
                }
            },
            Command::Sleep { at_ms, end_of_track } => {
                self.sleep = (at_ms.is_some() || end_of_track).then_some(Sleep { at_ms, end_of_track });
                self.sleep_place = end_of_track.then_some(self.now.index);
            }
            // Not Kodi's: Radio and Autoplay (the app's), measurements; handled at Cratarr: the rest.
            _ => {}
        }
        Ok(())
    }

    async fn seek(&self, ms: u64) -> Result<()> {
        self.rpc.call("Player.Seek", json!({ "playerid": PLAYER, "value": { "time": kodi_time(ms) } })).await.map(|_| ())
    }

    /// What Kodi plays now.
    async fn read(&self) -> Result<Now> {
        let volume = self
            .rpc
            .call("Application.GetProperties", json!({ "properties": ["volume"] }))
            .await?
            .get("volume")
            .and_then(Value::as_u64)
            .unwrap_or(100)
            .min(100) as u8;
        let players = self.rpc.call("Player.GetActivePlayers", json!({})).await?;
        let music = players.as_array().into_iter().flatten().any(|p| p.get("playerid").and_then(Value::as_u64) == Some(PLAYER));
        let mut now = Now { index: -1, volume, repeat: self.now.repeat, ..Now::default() };
        if !music {
            return Ok(now);
        }
        let p = self
            .rpc
            .call("Player.GetProperties", json!({ "playerid": PLAYER, "properties": ["position", "time", "totaltime", "speed", "repeat"] }))
            .await?;
        let item = self.rpc.call("Player.GetItem", json!({ "playerid": PLAYER, "properties": ["file"] })).await?;
        let file = item.pointer("/item/file").and_then(Value::as_str).unwrap_or_default();
        if self.following.is_some() {
            now.playing = file.contains("/listen.wav");
            return Ok(now);
        }
        let index = p.get("position").and_then(Value::as_i64).unwrap_or(-1);
        // Its address without the signature (decision 0043), which differs each time one is handed out.
        let unsigned = file.split('?').next().unwrap_or(file);
        let ours = usize::try_from(index).ok().and_then(|i| self.queue.get(i)).is_some_and(|t| address(&self.stream_from, t) == unsigned);
        if !ours {
            return Ok(now);
        }
        now.ours = true;
        now.index = index;
        now.playing = p.get("speed").and_then(Value::as_f64).unwrap_or(0.0) > 0.0;
        now.position_ms = p.get("time").map(ms_of).unwrap_or(0);
        now.duration_ms = p.get("totaltime").map(ms_of).filter(|d| *d > 0);
        now.repeat = match p.get("repeat").and_then(Value::as_str) {
            Some("one") => Repeat::One,
            Some("all") => Repeat::All,
            _ => Repeat::Off,
        };
        Ok(now)
    }

    /// The sleep timer, looked at with each read: its time come, or the track it waited for over.
    async fn keep_sleep(&mut self) {
        let Some(s) = self.sleep else { return };
        let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        let due = s.at_ms.is_some_and(|at| at <= now_ms) || (s.end_of_track && self.sleep_place.is_some_and(|p| p != self.now.index));
        if due && self.now.playing {
            let _ = self.pause(json!(false)).await;
        }
        if due {
            self.sleep = None;
            self.sleep_place = None;
        }
    }

    fn report(&mut self) -> Report {
        let n = &self.now;
        let with_queue = std::mem::take(&mut self.queue_changed);
        Report {
            index: if n.ours { n.index } else { -1 },
            entry: if n.ours { usize::try_from(n.index).ok().and_then(|i| self.queue.get(i)).map(|t| t.entry.clone()) } else { None },
            version: self.version,
            repeat: Some(n.repeat),
            sleep: Some(self.sleep.unwrap_or_default()),
            position_ms: n.position_ms,
            duration_ms: n.duration_ms,
            playing: n.ours && n.playing,
            volume: self.unducked.unwrap_or(n.volume),
            ducked: self.unducked.is_some(),
            queue: with_queue.then(|| self.queue.clone()),
            ..Report::default()
        }
    }
}

/// The group's silence when it changes; never without a group.
async fn changed(m: &mut Option<tokio::sync::watch::Receiver<bool>>) -> Option<bool> {
    match m {
        Some(m) => match m.changed().await {
            Ok(()) => Some(*m.borrow()),
            Err(_) => std::future::pending().await,
        },
        None => std::future::pending().await,
    }
}

/// Drives one Kodi for as long as Cratarr runs (the task is aborted when the settings change).
pub async fn drive(play_on: Weak<PlayOn>, cfg: KodiConfig) {
    let rpc = Rpc::new(&cfg);
    let mut warned = false;
    loop {
        if let Err(e) = rpc.call("JSONRPC.Ping", json!({})).await {
            if !warned {
                let error = format!("{e:#}");
                tracing::warn!(target: "playon", player = cfg.name.as_str(), error, "out of reach");
                warned = true;
            }
            tokio::time::sleep(Duration::from_secs(10)).await;
            continue;
        }
        warned = false;
        let Some(me) = play_on.upgrade() else { return };
        let Ok(mut commands) = me.connect_kodi(&cfg.name, cfg.owner.as_deref()) else { return };
        // The queue Cratarr holds for it: what Kodi's playlist was filled with before Cratarr started again.
        let queue = me.get(&cfg.name).and_then(|v| v.queue).unwrap_or_default();
        drop(me);
        let mut d = Driver::new(&cfg, queue);
        let mut misses = 0;
        // The next read: a deadline of its own, so the fill's chunks (or anything else) do not put it off.
        let mut read_at = tokio::time::Instant::now();
        let mut silence: Option<tokio::sync::watch::Receiver<bool>> = None;
        loop {
            // Following: the group's silence, kept to (its break silences Kodi as it silences the others).
            let leader = d.following.clone();
            if leader.is_none() {
                silence = None;
            } else if silence.is_none() {
                silence = leader.as_deref().and_then(|l| crate::snapcast::Snapcast::instance()?.muted_of(l));
                if let Some(m) = &silence {
                    let mute = *m.borrow();
                    let _ = d.rpc.call("Application.SetMute", json!({ "mute": mute })).await;
                }
            }
            tokio::select! {
                Some(mute) = changed(&mut silence) => {
                    let _ = d.rpc.call("Application.SetMute", json!({ "mute": mute })).await;
                }
                c = commands.next() => {
                    let Some(text) = c else { break };
                    let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
                    if let Some(ver) = v.get("version").and_then(Value::as_u64) {
                        d.version = Some(ver);
                    }
                    match serde_json::from_value::<Command>(v) {
                        Ok(c) => {
                            if let Err(e) = d.run(c).await {
                                let error = format!("{e:#}");
                                tracing::warn!(target: "playon", player = d.name.as_str(), error, "command failed");
                            }
                        }
                        Err(e) => {
                            let error = e.to_string();
                            tracing::debug!(target: "playon", player = d.name.as_str(), error, "command not understood");
                        }
                    }
                    // Read at once: what the command did is reported now.
                    read_at = tokio::time::Instant::now() + Duration::from_millis(300);
                }
                _ = tokio::time::sleep_until(d.fill_at), if d.filling() => {
                    if let Err(e) = d.fill_step(usize::MAX).await {
                        let error = format!("{e:#}");
                        tracing::warn!(target: "playon", player = d.name.as_str(), error, "cannot fill its playlist");
                        d.fill_at = tokio::time::Instant::now() + Duration::from_secs(10);
                    }
                }
                _ = tokio::time::sleep_until(read_at) => {
                    match d.read().await {
                        Ok(now) => {
                            misses = 0;
                            d.now = now;
                            if d.following.is_some() {
                                if d.now.playing {
                                    d.keep_in_line().await;
                                } else if d.opened.is_none_or(|o| o.elapsed() > Duration::from_secs(3)) {
                                    // Stopped (its format changed, a network stall): the group's sound again.
                                    let _ = d.open_group().await;
                                }
                            }
                            d.keep_sleep().await;
                            let report = d.report();
                            let Some(me) = play_on.upgrade() else { return };
                            // Off the async threads: a new track's ids are SQLite (0.10.2: the slow answers of 2026-10-30).
                            let _ = me.report_off(d.name.clone(), report).await;
                        }
                        Err(e) => {
                            misses += 1;
                            if misses >= MISSES {
                                let error = format!("{e:#}");
                                tracing::warn!(target: "playon", player = d.name.as_str(), error, "stopped answering");
                                break;
                            }
                        }
                    }
                    read_at = tokio::time::Instant::now() + if d.now.playing || d.following.is_some() { POLL_PLAYING } else { POLL_IDLE };
                }
            }
        }
        // Dropped: offline until it answers again.
        drop(commands);
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, Router, routing::post};
    use std::sync::Arc;
    use std::sync::Mutex;

    /// A Kodi that writes down what it is asked and answers as Kodi does, playing what it was told to.
    #[derive(Default)]
    struct FakeKodi {
        calls: Vec<(String, Value)>,
        playlist: Vec<String>,
        position: i64,
        speed: f64,
    }

    async fn fake() -> (String, Arc<Mutex<FakeKodi>>) {
        let k = Arc::new(Mutex::new(FakeKodi::default()));
        let state = k.clone();
        let app = Router::new().route(
            "/jsonrpc",
            post(move |Json(req): Json<Value>| {
                let k = state.clone();
                async move {
                    let mut k = k.lock().unwrap();
                    let (method, params) = (req["method"].as_str().unwrap_or_default().to_string(), req["params"].clone());
                    k.calls.push((method.clone(), params.clone()));
                    let result = match method.as_str() {
                        "Playlist.Clear" => {
                            k.playlist.clear();
                            json!("OK")
                        }
                        "Playlist.Add" => {
                            for i in params["item"].as_array().cloned().unwrap_or_default() {
                                k.playlist.push(i["file"].as_str().unwrap().to_string());
                            }
                            json!("OK")
                        }
                        "Playlist.Insert" => {
                            let at = params["position"].as_u64().unwrap() as usize;
                            k.playlist.insert(at, params["item"]["file"].as_str().unwrap().to_string());
                            json!("OK")
                        }
                        "Playlist.Remove" => {
                            let at = params["position"].as_u64().unwrap() as usize;
                            k.playlist.remove(at);
                            json!("OK")
                        }
                        "Playlist.Swap" => {
                            let (a, b) = (params["position1"].as_u64().unwrap() as usize, params["position2"].as_u64().unwrap() as usize);
                            k.playlist.swap(a, b);
                            json!("OK")
                        }
                        "Player.Open" => {
                            k.position = params["item"]["position"].as_i64().unwrap_or(0);
                            k.speed = 1.0;
                            json!("OK")
                        }
                        "Player.PlayPause" => {
                            k.speed = match &params["play"] {
                                Value::Bool(b) => f64::from(u8::from(*b)),
                                _ => 1.0 - k.speed,
                            };
                            json!({ "speed": k.speed })
                        }
                        "Player.GetActivePlayers" => json!([{ "playerid": 0, "type": "audio" }]),
                        "Player.GetProperties" => json!({
                            "position": k.position, "speed": k.speed, "repeat": "off",
                            "time": { "hours": 0, "minutes": 1, "seconds": 2, "milliseconds": 500 },
                            "totaltime": { "hours": 0, "minutes": 4, "seconds": 0, "milliseconds": 0 },
                        }),
                        "Player.GetItem" => json!({ "item": { "file": k.playlist.get(k.position as usize).cloned().unwrap_or_default() } }),
                        "Application.GetProperties" => json!({ "volume": 80 }),
                        _ => json!("OK"),
                    };
                    Json(json!({ "jsonrpc": "2.0", "id": req["id"], "result": result }))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (url, k)
    }

    fn t(id: &str) -> QueueTrack {
        QueueTrack { entry: format!("e-{id}"), source: "cratarr".into(), id: id.into(), name: id.into(), ..QueueTrack::default() }
    }

    #[tokio::test]
    async fn cratarrs_commands_play_on_kodi_and_its_state_comes_back() {
        let (url, kodi) = fake().await;
        let cfg = KodiConfig { name: "Bedroom".into(), url: format!("{url}/jsonrpc"), stream_from: "http://192.168.1.5:8081".into(), ..KodiConfig::default() };
        let mut d = Driver::new(&cfg, Vec::new());
        d.run(Command::play(vec![t("tr-1"), t("tr-2"), t("tr-3")], 1, 0)).await.unwrap();
        assert_eq!(kodi.lock().unwrap().playlist, vec![
            "http://192.168.1.5:8081/api/file/cratarr/tr-1",
            "http://192.168.1.5:8081/api/file/cratarr/tr-2",
            "http://192.168.1.5:8081/api/file/cratarr/tr-3",
        ]);
        d.now = d.read().await.unwrap();
        let r = d.report();
        assert_eq!((r.index, r.entry.as_deref(), r.playing, r.position_ms, r.duration_ms, r.volume), (1, Some("e-tr-2"), true, 62_500, Some(240_000), 80));
        assert_eq!(r.queue.map(|q| q.len()), Some(3), "the queue goes with the first report after a change");
        assert_eq!(d.report().queue, None);

        // Edits by entry: one after the playing track, the last to the front, the first out.
        d.run(Command::Add { from: Default::default(), tracks: vec![t("tr-9")], at: None, after: Some("e-tr-2".into()), how: None }).await.unwrap();
        d.run(Command::Move { from: None, to: None, entry: Some("e-tr-3".into()), after: Some(String::new()) }).await.unwrap();
        d.run(Command::Remove { index: None, entry: Some("e-tr-1".into()) }).await.unwrap();
        let files: Vec<String> = kodi.lock().unwrap().playlist.iter().map(|f| f.rsplit('/').next().unwrap().to_string()).collect();
        assert_eq!(files, vec!["tr-3", "tr-2", "tr-9"]);
        assert_eq!(d.queue.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), vec!["tr-3", "tr-2", "tr-9"]);

        d.run(Command::Pause).await.unwrap();
        d.run(Command::Seek { position_ms: Some(90_000), percent: None, by_ms: None, wind: false }).await.unwrap();
        let calls = kodi.lock().unwrap().calls.clone();
        let seek = calls.iter().rev().find(|(m, _)| m == "Player.Seek").unwrap();
        assert_eq!(seek.1["value"]["time"], json!({ "hours": 0, "minutes": 1, "seconds": 30, "milliseconds": 0 }));
        assert!(calls.iter().any(|(m, p)| m == "Player.PlayPause" && p["play"] == json!(false)));

        // Kodi playing something of its own (a film): nothing of Cratarr's plays.
        kodi.lock().unwrap().playlist[0] = "smb://films/x.mkv".into();
        kodi.lock().unwrap().position = 0;
        d.now = d.read().await.unwrap();
        assert_eq!((d.now.ours, d.report().index), (false, -1));

        // Into a group: its own music stops, the group's sound plays.
        d.run(Command::Group { leader: Some("Salon tablet".into()), host: None, port: 1704, buffer_ms: 1000, secret: String::new() }).await.unwrap();
        let k = kodi.lock().unwrap();
        assert!(k.playlist.is_empty());
        let open = k.calls.iter().rev().find(|(m, _)| m == "Player.Open").unwrap();
        assert_eq!(open.1["item"]["file"], "http://192.168.1.5:8081/api/groups/Salon%20tablet/listen.wav?player=Bedroom");
    }

    #[tokio::test]
    async fn a_long_queue_plays_at_once_and_the_rest_follows() {
        let (url, kodi) = fake().await;
        let cfg = KodiConfig { name: "Bedroom".into(), url, stream_from: "http://192.168.1.5:8081".into(), ..KodiConfig::default() };
        let mut d = Driver::new(&cfg, Vec::new());
        d.run(Command::play((0..472).map(|i| t(&format!("tr-{i}"))).collect(), 100, 0)).await.unwrap();
        {
            // Up to a few past the track it starts with, in chunks, then it plays.
            let k = kodi.lock().unwrap();
            let adds: Vec<usize> = k.calls.iter().filter(|(m, _)| m == "Playlist.Add").map(|(_, p)| p["item"].as_array().unwrap().len()).collect();
            assert_eq!((adds, k.playlist.len(), k.position), (vec![50, 50, 4], 104, 100));
        }
        d.now = d.read().await.unwrap();
        assert_eq!((d.now.ours, d.now.index), (true, 100));
        assert!(d.filling());

        // Past what Kodi holds, an added track waits for the fill; new upcoming tracks replace what it held after the
        // playing one.
        d.run(Command::Add { from: Default::default(), tracks: vec![t("tr-new")], at: None, after: Some("e-tr-200".into()), how: None }).await.unwrap();
        assert!(!kodi.lock().unwrap().calls.iter().any(|(m, _)| m == "Playlist.Insert"));
        d.run(Command::Upcoming { tracks: (0..60).map(|i| t(&format!("up-{i}"))).collect(), shuffled: None }).await.unwrap();
        assert_eq!((d.queue.len(), kodi.lock().unwrap().playlist.len()), (161, 101));

        // The fill, a chunk at a time between the reads: Kodi's playlist is the queue.
        while d.filling() {
            d.fill_step(usize::MAX).await.unwrap();
        }
        let files: Vec<String> = kodi.lock().unwrap().playlist.iter().map(|f| f.rsplit('/').next().unwrap().to_string()).collect();
        assert_eq!(files, d.queue.iter().map(|t| t.id.clone()).collect::<Vec<_>>());
        assert_eq!(&files[99..102], ["tr-99", "tr-100", "up-0"]);
    }

    /// Against a real Kodi, muted throughout (`cargo test -- --ignored real_kodi`, with KODI_URL, KODI_USER, KODI_PASS,
    /// STREAM_FROM and TRACKS, a comma-separated list of Cratarr's track ids, in the environment).
    #[tokio::test]
    #[ignore]
    async fn real_kodi() {
        let env = |k: &str| std::env::var(k).unwrap_or_default();
        let cfg = KodiConfig { name: "Test".into(), url: env("KODI_URL"), user: env("KODI_USER"), password: env("KODI_PASS"), stream_from: env("STREAM_FROM"), owner: None };
        let mut d = Driver::new(&cfg, Vec::new());
        let was = d.rpc.call("Application.GetProperties", json!({ "properties": ["muted", "volume"] })).await.unwrap();
        println!("kodi before: {was}");
        d.rpc.call("Application.SetMute", json!({ "mute": true })).await.unwrap();
        let tracks: Vec<QueueTrack> = env("TRACKS").split(',').map(t).collect();
        let step = |s: &str, d: &Driver| println!("{s}: ours {} index {} playing {} at {} ms of {:?}", d.now.ours, d.now.index, d.now.playing, d.now.position_ms, d.now.duration_ms);
        d.run(Command::play(tracks, 0, 30_000)).await.unwrap();
        tokio::time::sleep(Duration::from_secs(3)).await;
        d.now = d.read().await.unwrap();
        step("play from 30 s", &d);
        d.run(Command::Next).await.unwrap();
        tokio::time::sleep(Duration::from_secs(3)).await;
        d.now = d.read().await.unwrap();
        step("next", &d);
        d.run(Command::Seek { position_ms: Some(60_000), percent: None, by_ms: None, wind: false }).await.unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
        d.now = d.read().await.unwrap();
        step("seek to 60 s", &d);
        d.run(Command::Pause).await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        d.now = d.read().await.unwrap();
        step("pause", &d);
        d.run(Command::Stop { moved_to: None }).await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        d.now = d.read().await.unwrap();
        step("stop", &d);
        let muted = was.get("muted").and_then(Value::as_bool).unwrap_or(false);
        d.rpc.call("Application.SetMute", json!({ "mute": muted })).await.unwrap();
    }

    #[test]
    fn addresses_and_times() {
        assert_eq!(address("http://h:8081/", &t("tr 1")), "http://h:8081/api/file/cratarr/tr%201");
        // A Bandcamp preview from Cratarr too, never from Bandcamp (decision 0071, amended).
        let preview = QueueTrack { source: "bandcamp".into(), id: crate::bandcamp::track_id("https://a.bandcamp.com/album/b", 1), ..QueueTrack::default() };
        assert_eq!(address("http://h:8081", &preview), format!("http://h:8081/api/file/bandcamp/{}", preview.id));
        assert_eq!(ms_of(&kodi_time(3_723_456)), 3_723_456);
    }
}
