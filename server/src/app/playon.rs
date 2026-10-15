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

/// A player as Home Assistant and the pages see it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlayerView {
    pub name: String,
    /// Connected now: commands reach it.
    pub online: bool,
    /// When it was last heard from (epoch seconds).
    pub seen: u64,
    pub playing: bool,
    pub index: i64,
    /// Now: counted on from the last report while it plays.
    pub position_ms: u64,
    pub duration_ms: Option<u64>,
    pub volume: u8,
    pub ducked: bool,
    pub queue_length: usize,
    /// The track at `index`.
    pub track: Option<QueueTrack>,
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
    report: Report,
    queue: Vec<QueueTrack>,
    /// When `report` came.
    at: Instant,
}

impl Entry {
    fn view(&self, name: &str, with_queue: bool) -> PlayerView {
        let r = &self.report;
        let mut position_ms = r.position_ms;
        if r.playing {
            position_ms += self.at.elapsed().as_millis() as u64;
            if let Some(d) = r.duration_ms {
                position_ms = position_ms.min(d);
            }
        }
        PlayerView {
            name: name.to_string(),
            online: self.connection.is_some(),
            seen: self.seen.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
            playing: r.playing,
            index: r.index,
            position_ms,
            duration_ms: r.duration_ms,
            volume: r.volume,
            ducked: r.ducked,
            queue_length: self.queue.len(),
            track: usize::try_from(r.index).ok().and_then(|i| self.queue.get(i)).cloned(),
            queue: with_queue.then(|| self.queue.clone()),
        }
    }
}

pub struct PlayOn {
    entries: Mutex<HashMap<String, Entry>>,
    changed: broadcast::Sender<()>,
    connections: AtomicU64,
    settings: Mutex<PlayOnSettings>,
    file: Option<PathBuf>,
    playlists: Arc<PlaylistService>,
}

impl PlayOn {
    pub fn new(playlists: Arc<PlaylistService>, data_dir: Option<&str>) -> Arc<Self> {
        let file = data_dir.map(|d| PathBuf::from(d).join("players.json"));
        let settings = file.as_deref().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let me = Arc::new(Self {
            entries: Mutex::default(),
            changed: broadcast::channel(16).0,
            connections: AtomicU64::new(0),
            settings: Mutex::new(settings),
            file,
            playlists,
        });
        me.clone().tell_webhooks();
        me
    }

    fn entries(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
        self.entries.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn changed(&self) {
        let _ = self.changed.send(());
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
            let e = entries.entry(name.to_string()).or_insert_with(|| Entry {
                connection: None,
                seen: SystemTime::now(),
                report: Report { index: -1, volume: 100, ..Report::default() },
                queue: Vec::new(),
                at: Instant::now(),
            });
            e.connection = Some((id, tx));
            e.seen = SystemTime::now();
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
        let e = entries.entry(name.to_string()).or_insert_with(|| Entry {
            connection: None,
            seen: SystemTime::now(),
            report: Report::default(),
            queue: Vec::new(),
            at: Instant::now(),
        });
        let before = e.view(name, false);
        e.seen = SystemTime::now();
        if let Some(q) = report.queue.clone() {
            e.queue = q;
        }
        e.report = Report { queue: None, ..report };
        e.at = Instant::now();
        let after = e.view(name, false);
        drop(entries);
        // A position that moves on as counted is no news; a jump (a seek) is.
        let jumped = before.position_ms.abs_diff(after.position_ms) > 2_000;
        let other = PlayerView { position_ms: 0, seen: 0, ..before } != PlayerView { position_ms: 0, seen: 0, ..after };
        if other || jumped {
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
                drop(me);
                for url in hooks {
                    if let Err(e) = http.post(&url).json(&serde_json::json!({ "players": players })).send().await.and_then(|r| r.error_for_status()) {
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
