//! Play On (decision 0031): the players Cratarr commands, and what each plays.
//!
//! A player is the Android app (android/). It keeps `GET /api/players/{name}/connect` open (server-sent events: the
//! commands for it) and reports what it plays with `PUT /api/players/{name}/state`. Home Assistant, or anything else,
//! reads `GET /api/players`, commands a player with `POST /api/players/{name}/command`, and is told of every change:
//! the player list is POSTed to each webhook (`GET`/`PUT /api/players/settings`, kept in `players.json`).
//!
//! Players live in memory: one whose connection is closed is offline; one not heard of since the start is unknown.

use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};

use super::{PlaylistService, ToggleError};

/// A player not heard from (a report, the 30 s heartbeat) for this long is offline, even if its connection still looks
/// open: a device that lost power or the network leaves the connection open until a write fails, minutes later.
const LEASE: Duration = Duration::from_secs(75);
/// Changes reaching the webhooks within this long go out as one.
const SETTLE: Duration = Duration::from_millis(300);
/// How long a webhook may take to answer.
const WEBHOOK_TIMEOUT: Duration = Duration::from_secs(5);

/// A track as players hold it: the page's own description (`LocalTrack` in web/src/lib/localPlayer.svelte.ts).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QueueTrack {
    pub source: String,
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub album_id: Option<String>,
}

/// What a player reports.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Report {
    /// The playing track's place in the queue; -1 with nothing loaded.
    pub index: i64,
    pub position_ms: u64,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    pub playing: bool,
    /// 0 to 100.
    #[serde(default = "full_volume")]
    pub volume: u8,
    #[serde(default)]
    pub ducked: bool,
    /// The whole queue when it changed since the last report; `None` keeps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue: Option<Vec<QueueTrack>>,
}

fn full_volume() -> u8 {
    100
}

/// What can be asked of a player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    /// A playlist (by name) or tracks, from track `index`, `position_ms` into it; shuffled first when asked (then from
    /// the start). What reaches the player is always `tracks`.
    Play {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        playlist: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tracks: Option<Vec<QueueTrack>>,
        #[serde(default)]
        index: usize,
        #[serde(default)]
        position_ms: u64,
        #[serde(default, skip_serializing)]
        shuffle: bool,
    },
    /// Tracks into the queue at `at` (the end when left out).
    Add {
        tracks: Vec<QueueTrack>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<usize>,
    },
    /// Queue track `from` to place `to`.
    Move { from: usize, to: usize },
    /// Queue track `index` out.
    Remove { index: usize },
    /// To queue track `index`, from its start.
    Jump { index: usize },
    Pause,
    Resume,
    Toggle,
    Stop,
    Next,
    Previous,
    /// To `position_ms`, or `percent` (0 to 100) of the track. What reaches the player is always `position_ms`.
    Seek {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        position_ms: Option<u64>,
        #[serde(default, skip_serializing)]
        percent: Option<f64>,
    },
    /// 0 to 100.
    Volume { level: u8 },
    /// Quieter, for an announcement; `Unduck` brings it back.
    Duck,
    Unduck,
    /// The queue and position move to player `to`, and this one pauses. Handled here; never sent to a player.
    Handover { to: String },
}

/// Cratarr's own ids of the playing track (decision 0030): what Home Assistant's library calls take.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TrackIds {
    pub track_id: Option<String>,
    pub album_id: Option<String>,
    pub album_artist: Option<String>,
}

/// A player as Home Assistant and the pages see it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlayerView {
    pub name: String,
    /// Connected now and heard from lately: commands reach it.
    pub online: bool,
    /// `playing`, `paused` or `stopped` (nothing loaded).
    pub state: &'static str,
    /// When it was last heard from (epoch seconds).
    pub seen: u64,
    pub playing: bool,
    pub index: i64,
    /// Now: counted on from the last report while it plays.
    pub position_ms: u64,
    /// When `position_ms` was so (epoch ms): from then on it moves with the music while `playing`.
    pub position_at: u64,
    pub duration_ms: Option<u64>,
    pub volume: u8,
    pub ducked: bool,
    pub queue_length: usize,
    /// Goes up whenever the queue changes: ask for the player by name to see the new one.
    pub queue_version: u64,
    /// The track at `index`.
    pub track: Option<QueueTrack>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// Cratarr's own ids of the track at `index` (and its album artist).
    #[serde(flatten)]
    pub ids: TrackIds,
    /// The queue entry that plays; `null` until queue entries have ids of their own (decision 0033).
    pub current_entry_id: Option<String>,
    /// The whole queue, for one player asked for by name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue: Option<Vec<QueueTrack>>,
}

/// Saved in `players.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayOnSettings {
    /// Addresses the player list is POSTed to on every change (`{"players": [...]}`).
    #[serde(default)]
    pub webhooks: Vec<String>,
}

struct Entry {
    /// The open connection, as (its number, where its commands go).
    connection: Option<(u64, mpsc::Sender<String>)>,
    seen: SystemTime,
    /// When it was last heard from, for the lease.
    heard: Instant,
    /// Cratarr's ids of the track at `report.index`, looked up when the track changes.
    ids: TrackIds,
    report: Report,
    queue: Vec<QueueTrack>,
    queue_version: u64,
    /// When `report` came.
    at: Instant,
}

impl Entry {
    fn new() -> Self {
        Self {
            connection: None,
            seen: SystemTime::now(),
            heard: Instant::now(),
            ids: TrackIds::default(),
            report: Report { index: -1, volume: 100, ..Report::default() },
            queue: Vec::new(),
            queue_version: 0,
            at: Instant::now(),
        }
    }

    fn online(&self) -> bool {
        self.connection.is_some() && self.heard.elapsed() < LEASE
    }

    fn view(&self, name: &str, with_queue: bool) -> PlayerView {
        let r = &self.report;
        let mut position_ms = r.position_ms;
        if r.playing {
            position_ms += self.at.elapsed().as_millis() as u64;
            if let Some(d) = r.duration_ms {
                position_ms = position_ms.min(d);
            }
        }
        let track = usize::try_from(r.index).ok().and_then(|i| self.queue.get(i)).cloned();
        PlayerView {
            name: name.to_string(),
            online: self.online(),
            state: if track.is_none() {
                "stopped"
            } else if r.playing {
                "playing"
            } else {
                "paused"
            },
            seen: self.seen.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
            playing: r.playing,
            index: r.index,
            position_ms,
            position_at: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
            duration_ms: r.duration_ms,
            volume: r.volume,
            ducked: r.ducked,
            queue_length: self.queue.len(),
            queue_version: self.queue_version,
            title: track.as_ref().map(|t| t.name.clone()),
            artist: track.as_ref().and_then(|t| t.artist.clone()),
            album: track.as_ref().and_then(|t| t.album.clone()),
            ids: self.ids.clone(),
            current_entry_id: None,
            track,
            queue: with_queue.then(|| self.queue.clone()),
        }
    }
}

pub struct PlayOn {
    entries: Mutex<HashMap<String, Entry>>,
    changed: broadcast::Sender<()>,
    connections: AtomicU64,
    /// Numbers the webhook POSTs: a receiver drops one older than the last it took.
    seq: AtomicU64,
    lib: Arc<super::Library>,
    /// Cratarr's own library (0030), for the tracks' ids; set at the start (`set_library`).
    library: std::sync::OnceLock<Arc<crate::serve::library::Library>>,
    settings: Mutex<PlayOnSettings>,
    file: Option<PathBuf>,
    playlists: Arc<PlaylistService>,
}

impl PlayOn {
    pub fn new(lib: Arc<super::Library>, playlists: Arc<PlaylistService>, data_dir: Option<&str>) -> Arc<Self> {
        let file = data_dir.map(|d| PathBuf::from(d).join("players.json"));
        let settings = file.as_deref().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let me = Arc::new(Self {
            entries: Mutex::default(),
            changed: broadcast::channel(16).0,
            connections: AtomicU64::new(0),
            seq: AtomicU64::new(0),
            lib,
            library: std::sync::OnceLock::new(),
            settings: Mutex::new(settings),
            file,
            playlists,
        });
        me.clone().tell_webhooks();
        me.clone().watch_leases();
        me
    }

    /// Cratarr's own library, once open: the players' tracks get its ids.
    pub fn set_library(&self, library: Arc<crate::serve::library::Library>) {
        let _ = self.library.set(library);
    }

    /// Cratarr's ids of a player's track (`None` parts when the library does not know it).
    fn ids_of(&self, t: Option<&QueueTrack>) -> TrackIds {
        let (Some(t), Some(library)) = (t, self.library.get()) else { return TrackIds::default() };
        let Some(track) = self.lib.catalogue().track_of_source(&t.source, &t.id) else { return TrackIds::default() };
        match library.song_ids(track) {
            Ok(Some(ids)) => TrackIds { track_id: Some(ids.song_id), album_id: Some(ids.album_id), album_artist: Some(ids.album_artist) },
            _ => TrackIds { track_id: Some(crate::serve::ids::song(track)), ..TrackIds::default() },
        }
    }

    /// A player whose lease runs out goes offline without a word from it: looked at every 15 s.
    fn watch_leases(self: Arc<Self>) {
        let me = Arc::downgrade(&self);
        drop(self);
        tokio::spawn(async move {
            let mut was: HashMap<String, bool> = HashMap::new();
            loop {
                tokio::time::sleep(Duration::from_secs(15)).await;
                let Some(me) = me.upgrade() else { return };
                let now: HashMap<String, bool> = me.entries().iter().map(|(n, e)| (n.clone(), e.online())).collect();
                if now.iter().any(|(n, on)| was.get(n).is_some_and(|w| w != on)) {
                    me.changed();
                }
                was = now;
            }
        });
    }

    fn entries(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
        self.entries.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn changed(&self) {
        let _ = self.changed.send(());
    }

    /// Told on every change of a player (the pages' live channel).
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.changed.subscribe()
    }

    /// Every player Cratarr has heard of since it started, by name.
    pub fn list(&self) -> Vec<PlayerView> {
        let mut out: Vec<PlayerView> = self.entries().iter().map(|(n, e)| e.view(n, false)).collect();
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        out
    }

    /// One player, with its queue.
    pub fn get(&self, name: &str) -> Option<PlayerView> {
        self.entries().get(name).map(|e| e.view(name, true))
    }

    /// A player connects: the commands for it come out of the returned stream, which closes the connection when it
    /// is dropped (the player went away). A second connection under the same name replaces the first.
    pub fn connect(self: &Arc<Self>, name: &str) -> Result<Commands, ToggleError> {
        valid_name(name)?;
        let (tx, rx) = mpsc::channel(16);
        let id = self.connections.fetch_add(1, Ordering::Relaxed);
        {
            let mut entries = self.entries();
            let e = entries.entry(name.to_string()).or_insert_with(Entry::new);
            e.connection = Some((id, tx));
            e.seen = SystemTime::now();
            e.heard = Instant::now();
        }
        tracing::info!("player {name:?} connected");
        self.changed();
        Ok(Commands { rx, play_on: self.clone(), name: name.to_string(), id })
    }

    fn disconnected(&self, name: &str, id: u64) {
        let mut entries = self.entries();
        let Some(e) = entries.get_mut(name) else { return };
        if e.connection.as_ref().is_some_and(|(c, _)| *c == id) {
            e.connection = None;
            drop(entries);
            tracing::info!("player {name:?} disconnected");
            self.changed();
        }
    }

    /// What a player says it plays.
    pub fn report(&self, name: &str, report: Report) -> Result<(), ToggleError> {
        valid_name(name)?;
        let mut entries = self.entries();
        let e = entries.entry(name.to_string()).or_insert_with(Entry::new);
        let before = e.view(name, false);
        e.seen = SystemTime::now();
        e.heard = Instant::now();
        if let Some(q) = report.queue.clone()
            && q != e.queue
        {
            e.queue = q;
            e.queue_version += 1;
        }
        e.report = Report { queue: None, ..report };
        e.at = Instant::now();
        let track = e.view(name, false).track;
        let new_track = track != before.track;
        drop(entries);
        // A new track: its ids, looked up outside the lock.
        if new_track {
            let ids = self.ids_of(track.as_ref());
            if let Some(e) = self.entries().get_mut(name) {
                e.ids = ids;
            }
        }
        let after = match self.entries().get(name) {
            Some(e) => e.view(name, false),
            None => return Ok(()),
        };
        // A position that moves on as counted is no news; a jump (a seek) is.
        let jumped = before.position_ms.abs_diff(after.position_ms) > 2_000;
        let still = |v: &PlayerView| PlayerView { position_ms: 0, position_at: 0, seen: 0, ..v.clone() };
        if still(&before) != still(&after) || jumped {
            self.changed();
        }
        Ok(())
    }

    /// Asks player `name` to do something.
    pub async fn command(&self, name: &str, command: Command) -> Result<(), ToggleError> {
        let command = match command {
            Command::Handover { to } => return self.handover(name, &to).await,
            Command::Play { playlist, tracks, index, position_ms, shuffle } => {
                let (mut tracks, mut index, mut position_ms) = match (playlist, tracks) {
                    (Some(p), _) => (self.playlist_tracks(&p).await?, index, position_ms),
                    (None, Some(t)) => (t, index, position_ms),
                    (None, None) => return Err(ToggleError::Refused("Play what? Give a playlist or tracks.".into())),
                };
                if tracks.is_empty() {
                    return Err(ToggleError::Refused("There is nothing there that can be played.".into()));
                }
                if shuffle {
                    shuffled(&mut tracks);
                    (index, position_ms) = (0, 0);
                }
                Command::Play { playlist: None, tracks: Some(tracks), index, position_ms, shuffle: false }
            }
            Command::Seek { position_ms: None, percent: Some(p) } => {
                let duration = self.entries().get(name).and_then(|e| e.report.duration_ms);
                let Some(d) = duration else { return Err(ToggleError::Refused("The track's length is not known yet.".into())) };
                Command::Seek { position_ms: Some((d as f64 * p.clamp(0.0, 100.0) / 100.0) as u64), percent: None }
            }
            Command::Seek { position_ms: None, percent: None } => return Err(ToggleError::Refused("Seek where?".into())),
            other => other,
        };
        self.send(name, &command)
    }

    fn send(&self, name: &str, command: &Command) -> Result<(), ToggleError> {
        let json = serde_json::to_string(command).map_err(anyhow::Error::from)?;
        let entries = self.entries();
        let Some(e) = entries.get(name) else { return Err(ToggleError::Refused(format!("No player is called {name}."))) };
        let Some((_, tx)) = &e.connection else { return Err(ToggleError::Refused(format!("{name} is not connected."))) };
        tx.try_send(json).map_err(|_| ToggleError::Refused(format!("{name} does not take commands now.")))
    }

    /// The queue and position of `from` go to `to`, which plays them; `from` pauses.
    async fn handover(&self, from: &str, to: &str) -> Result<(), ToggleError> {
        let (tracks, index, position_ms) = {
            let entries = self.entries();
            let Some(e) = entries.get(from) else { return Err(ToggleError::Refused(format!("No player is called {from}."))) };
            let v = e.view(from, false);
            (e.queue.clone(), v.index.max(0) as usize, v.position_ms)
        };
        if tracks.is_empty() {
            return Err(ToggleError::Refused(format!("{from} has nothing to hand over.")));
        }
        self.send(to, &Command::Play { playlist: None, tracks: Some(tracks), index, position_ms, shuffle: false })?;
        // The music is on its way there; this one pausing is no reason to fail the handover.
        let _ = self.send(from, &Command::Pause);
        Ok(())
    }

    /// A playlist's playable tracks, as players hold them.
    async fn playlist_tracks(&self, playlist: &str) -> Result<Vec<QueueTrack>, ToggleError> {
        let (source, rows) = self.playlists.rows(playlist, None).await?;
        Ok(rows
            .into_iter()
            .filter(|r| !r.missing)
            .filter_map(|r| {
                Some(QueueTrack { source: source.clone(), id: r.id?, name: r.name.unwrap_or_default(), artist: r.artist, album: r.album, album_id: None })
            })
            .collect())
    }

    pub fn settings(&self) -> PlayOnSettings {
        self.settings.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub async fn save_settings(&self, draft: PlayOnSettings) -> Result<PlayOnSettings, ToggleError> {
        let Some(file) = self.file.clone() else { return Err(ToggleError::SettingsReadOnly) };
        let mut webhooks = Vec::new();
        for w in draft.webhooks.iter().map(|w| w.trim()).filter(|w| !w.is_empty()) {
            if !(w.starts_with("http://") || w.starts_with("https://")) || reqwest::Url::parse(w).is_err() {
                return Err(ToggleError::BadSettings(format!("{w} is not a web address (http:// or https://)")));
            }
            webhooks.push(w.to_string());
        }
        let saved = PlayOnSettings { webhooks };
        let body = serde_json::to_string_pretty(&saved).map_err(anyhow::Error::from)?;
        tokio::task::spawn_blocking(move || write_private(&file, &body)).await.map_err(|e| anyhow::anyhow!("{e}"))??;
        *self.settings.lock().unwrap_or_else(|p| p.into_inner()) = saved.clone();
        self.changed();
        Ok(saved)
    }

    /// POSTs the player list to every webhook after each change (changes close together go out as one).
    fn tell_webhooks(self: Arc<Self>) {
        let mut rx = self.changed.subscribe();
        let me = Arc::downgrade(&self);
        drop(self);
        tokio::spawn(async move {
            let http = reqwest::Client::builder().timeout(WEBHOOK_TIMEOUT).build().expect("a plain client builds");
            loop {
                // Behind (more changes than the channel holds) is a change too; closed is Cratarr stopping.
                if let Err(broadcast::error::RecvError::Closed) = rx.recv().await {
                    return;
                }
                tokio::time::sleep(SETTLE).await;
                while rx.try_recv().is_ok() {}
                let Some(me) = me.upgrade() else { return };
                let (hooks, players) = (me.settings().webhooks, me.list());
                let seq = me.seq.fetch_add(1, Ordering::Relaxed) + 1;
                drop(me);
                if hooks.is_empty() {
                    continue;
                }
                let body = serde_json::json!({ "seq": seq, "players": players });
                for url in hooks {
                    if let Err(e) = http.post(&url).json(&body).send().await.and_then(|r| r.error_for_status()) {
                        tracing::warn!("Play On webhook {url}: {e}");
                    }
                }
            }
        });
    }
}

/// The commands for one connected player; dropping it (the connection closed) marks the player offline.
pub struct Commands {
    rx: mpsc::Receiver<String>,
    play_on: Arc<PlayOn>,
    name: String,
    id: u64,
}

impl tokio_stream::Stream for Commands {
    type Item = String;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<String>> {
        self.rx.poll_recv(cx)
    }
}

impl Drop for Commands {
    fn drop(&mut self) {
        self.play_on.disconnected(&self.name, self.id);
    }
}

fn valid_name(name: &str) -> Result<(), ToggleError> {
    if name.trim().is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
        return Err(ToggleError::BadSettings("a player's name is 1 to 64 characters, no control characters".into()));
    }
    Ok(())
}

/// In place, in a random order (xorshift seeded from the clock: a shuffle needs no better).
fn shuffled<T>(items: &mut [T]) {
    let mut x = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) | 1;
    for i in (1..items.len()).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        items.swap(i, (x % (i as u64 + 1)) as usize);
    }
}

/// Written whole (a temporary file, then renamed), readable by the owner only: webhook addresses can carry secrets.
fn write_private(path: &std::path::Path, body: &str) -> Result<()> {
    use std::io::Write;
    let tmp = path.with_extension("json.tmp");
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp).with_context(|| format!("cannot write {}", tmp.display()))?;
    f.write_all(body.as_bytes())?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path).with_context(|| format!("cannot replace {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_read_as_home_assistant_sends_them_and_reach_the_player_resolved() {
        let c: Command = serde_json::from_str(r#"{"command":"play","playlist":"Chill","shuffle":true}"#).unwrap();
        assert_eq!(c, Command::Play { playlist: Some("Chill".into()), tracks: None, index: 0, position_ms: 0, shuffle: true });
        let c: Command = serde_json::from_str(r#"{"command":"seek","percent":50}"#).unwrap();
        assert_eq!(c, Command::Seek { position_ms: None, percent: Some(50.0) });
        assert_eq!(serde_json::from_str::<Command>(r#"{"command":"duck"}"#).unwrap(), Command::Duck);
        // To the player: tracks, never the playlist's name or the shuffle wish.
        let to_player = Command::Play { playlist: None, tracks: Some(vec![]), index: 2, position_ms: 5, shuffle: false };
        assert_eq!(serde_json::to_string(&to_player).unwrap(), r#"{"command":"play","tracks":[],"index":2,"position_ms":5}"#);
        assert_eq!(serde_json::to_string(&Command::Seek { position_ms: Some(9), percent: None }).unwrap(), r#"{"command":"seek","position_ms":9}"#);
    }

    #[test]
    fn a_shuffle_keeps_every_track() {
        let mut v: Vec<u32> = (0..50).collect();
        shuffled(&mut v);
        let mut s = v.clone();
        s.sort();
        assert_eq!(s, (0..50).collect::<Vec<_>>());
        assert_ne!(v, s, "50 tracks in their own order after a shuffle: one chance in 50!");
    }

    #[test]
    fn names_that_cannot_be_a_player() {
        assert!(valid_name("Salon tablet").is_ok());
        assert!(valid_name(" ").is_err());
        assert!(valid_name("a\nb").is_err());
        assert!(valid_name(&"x".repeat(65)).is_err());
    }
}
