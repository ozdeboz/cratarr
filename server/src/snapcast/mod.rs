//! Sync play (decision 0037, as amended): Cratarr speaks Snapcast's server protocol itself, so each device of a group
//! plays the group's sound with Snapcast's own player (snapclient), which plays every chunk at its time and drops or
//! repeats samples to stay there.
//!
//! The sound comes from the group's leader: its app plays the queue as it always does, its own speaker silent, and sends
//! what it plays here (a connection of its own, [`proto::SOURCE`]). Here it is stamped with the time it was played, on
//! Cratarr's steady clock, and sent on to every device of the group, the leader's own snapclient too, each playing it a
//! buffer later. A break in the leader's sound (a seek, a jump, a pause) silences the group at once and drops what was
//! still to come; the new sound plays a buffer after it starts.

pub mod listen;
pub mod proto;

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, watch};

use proto::Format;

/// Where Snapcast's clients connect by default.
pub const PORT: u16 = 1704;
/// From a chunk's stamp to its sound on every device: what a stall of the network may take before it is heard.
pub const BUFFER_MS: u32 = 1000;
/// Sound that comes this much later than its stamp says the leader stopped and went on (a stall): stamped anew.
const LATE_US: i64 = 500_000;
/// Sound held back longer than this (a leader that sends far ahead) is dropped, oldest first.
const MAX_HELD_US: i64 = 10_000_000;
/// The clock's start, so its readings are never near zero (snapserver's steady clock reads the time since boot).
const CLOCK_BASE_US: i64 = 1_000_000_000_000;

/// What the server needs of Play On: who plays in which group, and each one's volume.
pub trait Groups: Send + Sync + 'static {
    /// The leader of the group `player` plays in (itself when it leads one); `None` when it plays in none.
    fn leader_of(&self, player: &str) -> Option<String>;
    /// Its volume for the group's sound, 0 to 100 (ducked included).
    fn volume_of(&self, player: &str) -> u8;
    /// How late its speaker sounds (ms): its snapclient plays that much earlier.
    fn latency_of(&self, player: &str) -> i32;
    /// Whether `secret` is player `player`'s (decision 0051): it was told it with its group, and only it may listen to
    /// its group or play into it.
    fn admits(&self, player: &str, secret: &str) -> bool;
}

/// A snapclient's id as Cratarr's players give it (`--hostID`): their secret, a colon, their name. The secret has no
/// colon; the name may.
fn named(id: &str) -> (&str, &str) {
    id.split_once(':').unwrap_or(("", id))
}

/// What a group's stream sends its devices.
#[derive(Clone)]
enum Out {
    Codec(Arc<Vec<u8>>),
    Chunk(Arc<Vec<u8>>),
}

#[derive(Default)]
struct State {
    /// The format of the sound coming in.
    format: Option<Format>,
    /// The codec header sent last: a device that comes in gets it first.
    codec: Option<Arc<Vec<u8>>>,
    /// Waiting for their time: a chunk goes out at its stamp, so a break can still take back what comes after it.
    held: VecDeque<(i64, Out)>,
    /// The stamp the next sound gets; `None` after a break (the next sound is stamped as it comes).
    next: Option<i64>,
    broke_at: Option<i64>,
    /// After a break: when the group is heard again, once the new sound's first chunk is about to play.
    unmute_at: Option<i64>,
    /// The leader's source connection that feeds it; a newer one replaces it.
    source: u64,
}

/// A group's sound, by its leader.
struct Stream {
    state: Mutex<State>,
    out: broadcast::Sender<Out>,
    muted: watch::Sender<bool>,
    buffer_us: i64,
}

impl Stream {
    fn new(buffer_ms: u32) -> Self {
        Self { state: Mutex::default(), out: broadcast::channel(1024).0, muted: watch::channel(false).0, buffer_us: i64::from(buffer_ms) * 1000 }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// A source connection takes over: what the one before sent and not yet played goes.
    fn take_over(&self, source: u64, now: i64) {
        self.state().source = source;
        self.brk(source, now);
    }

    /// The leader's format. A new one goes out once the sound before it has played everywhere (a pause between them);
    /// the first at once.
    fn codec(&self, source: u64, f: Format, now: i64) {
        let mut s = self.state();
        if s.source != source || s.format == Some(f) {
            return;
        }
        let header = Out::Codec(Arc::new(proto::codec_header_pcm(f)));
        let playing_until = s.next.map(|n| n + self.buffer_us + 10_000);
        let at = match (s.format, playing_until) {
            (Some(_), Some(until)) => until.max(now),
            _ => now,
        };
        s.format = Some(f);
        s.held.push_back((at, header));
        if s.next.is_some() {
            s.next = Some(at);
        }
    }

    /// Sound from the leader, stamped where it follows the sound before it (anew after a break or a stall).
    fn push(&self, source: u64, audio: &[u8], now: i64) {
        let mut s = self.state();
        let Some(f) = s.format.filter(|_| s.source == source) else { return };
        let stamp = match s.next {
            Some(n) if now - n <= LATE_US => n,
            _ => now,
        };
        s.next = Some(stamp + f.duration_us(audio.len()));
        s.held.push_back((stamp, Out::Chunk(Arc::new(proto::wire_chunk(stamp, audio)))));
        // Silenced by a break: heard again as this sound begins, the old sound (silenced) played out by then.
        if *self.muted.borrow() && s.unmute_at.is_none() {
            let old_out = s.broke_at.map_or(0, |b| b + self.buffer_us + 5_000);
            s.unmute_at = Some((stamp + self.buffer_us - 20_000).max(old_out));
        }
        while s.held.len() > 1 && s.held.back().map(|b| b.0).unwrap_or(0) - s.held.front().map(|f| f.0).unwrap_or(0) > MAX_HELD_US {
            s.held.pop_front();
        }
    }

    /// The leader's sound broke off: the group goes silent now, and what was still to come goes (a new format still to
    /// go out goes now, as nothing old is heard any more).
    fn brk(&self, source: u64, now: i64) {
        let mut s = self.state();
        if s.source != source {
            return;
        }
        s.held.retain(|(_, o)| matches!(o, Out::Codec(_)));
        for h in s.held.iter_mut() {
            h.0 = now;
        }
        s.next = None;
        s.broke_at = Some(now);
        s.unmute_at = None;
        drop(s);
        self.muted.send_replace(true);
    }

    /// What is due by `now`, in order; heard again when it is time.
    fn release(&self, now: i64) -> Vec<Out> {
        let mut s = self.state();
        let mut due = Vec::new();
        while s.held.front().is_some_and(|(at, _)| *at <= now) {
            let (_, o) = s.held.pop_front().expect("looked at");
            if let Out::Codec(c) = &o {
                s.codec = Some(c.clone());
            }
            due.push(o);
        }
        let unmute = s.unmute_at.is_some_and(|t| t <= now);
        if unmute {
            s.unmute_at = None;
        }
        drop(s);
        if unmute {
            self.muted.send_replace(false);
        }
        due
    }
}

pub struct Snapcast {
    epoch: Instant,
    groups: Arc<dyn Groups>,
    streams: Mutex<HashMap<String, Arc<Stream>>>,
    sources: AtomicU64,
    buffer_ms: u32,
    /// Players without Snapcast's player (a Kodi), listening to their group's sound as WAV (`listen`), by name.
    listeners: Mutex<HashMap<String, Arc<listen::Listener>>>,
}

/// The one server, once it listens (a Kodi's driver finds its group's sound here).
static INSTANCE: std::sync::OnceLock<Arc<Snapcast>> = std::sync::OnceLock::new();

impl Snapcast {
    pub fn new(groups: Arc<dyn Groups>, buffer_ms: u32) -> Arc<Self> {
        Arc::new(Self { epoch: Instant::now(), groups, streams: Mutex::default(), sources: AtomicU64::new(1), buffer_ms, listeners: Mutex::default() })
    }

    /// The server, once Cratarr serves sync play. Requests take it from their `Ctx` (the app holds it); this stays for
    /// a Kodi's driver, a task of its own inside Play On, which is made before sync play (that is handed Play On).
    pub fn instance() -> Option<Arc<Self>> {
        INSTANCE.get().cloned()
    }

    pub fn set_instance(me: Arc<Self>) {
        let _ = INSTANCE.set(me);
    }

    /// From a chunk's stamp to its sound (ms).
    pub fn buffer_ms(&self) -> u32 {
        self.buffer_ms
    }

    /// Cratarr's steady clock (µs): what every stamp and every time exchange is in.
    pub fn now(&self) -> i64 {
        CLOCK_BASE_US + self.epoch.elapsed().as_micros() as i64
    }

    /// Takes connections on `listener` for as long as Cratarr runs.
    pub async fn serve(self: Arc<Self>, listener: TcpListener) {
        loop {
            match listener.accept().await {
                Ok((sock, peer)) => {
                    let me = self.clone();
                    tokio::spawn(async move {
                        if let Err(e) = me.connection(sock).await {
                            let (client, error) = (peer.ip().to_string(), e.to_string());
                            tracing::debug!(target: "syncplay", client, error, "connection failed");
                        }
                    });
                }
                Err(e) => {
                    let error = e.to_string();
                    tracing::warn!(target: "syncplay", error, "cannot take a connection");
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
    }

    /// A group's stream, made (with its clock) when first asked for.
    fn stream(self: &Arc<Self>, leader: &str) -> Arc<Stream> {
        let mut streams = self.streams.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(s) = streams.get(leader) {
            return s.clone();
        }
        let s = Arc::new(Stream::new(self.buffer_ms));
        streams.insert(leader.to_string(), s.clone());
        let (me, stream) = (Arc::downgrade(self), Arc::downgrade(&s));
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(5));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                let (Some(me), Some(stream)) = (me.upgrade(), stream.upgrade()) else { return };
                for o in stream.release(me.now()) {
                    let _ = stream.out.send(o);
                }
            }
        });
        s
    }

    async fn connection(self: Arc<Self>, sock: TcpStream) -> std::io::Result<()> {
        sock.set_nodelay(true)?;
        let (mut reader, writer) = sock.into_split();
        let now = || self.now();
        let (hello, payload) = tokio::time::timeout(Duration::from_secs(10), proto::read(&mut reader, now))
            .await
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "no hello"))??;
        let said = proto::read_json(&payload).unwrap_or_default();
        // Only a player with its secret (decision 0051): another device on the network named it, nothing more.
        let refused = |name: &str, kind: &str| {
            tracing::warn!(target: "syncplay", player = name, kind, "refused: not that player's secret");
            Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "not that player"))
        };
        match hello.kind {
            proto::HELLO => {
                let (secret, name) = named(said["ID"].as_str().unwrap_or_default());
                if !self.groups.admits(name, secret) {
                    return refused(name, "listener");
                }
                self.client(name.to_string(), hello, reader, writer).await
            }
            proto::SOURCE => {
                let name = said["player"].as_str().unwrap_or_default();
                if !self.groups.admits(name, said["secret"].as_str().unwrap_or_default()) {
                    return refused(name, "leader");
                }
                self.source(name.to_string(), reader).await
            }
            other => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("a first message of type {other}"))),
        }
    }

    /// A leader's sound, coming in.
    async fn source(self: Arc<Self>, name: String, mut reader: tokio::net::tcp::OwnedReadHalf) -> std::io::Result<()> {
        let id = self.sources.fetch_add(1, Ordering::Relaxed);
        let stream = self.stream(&name);
        stream.take_over(id, self.now());
        tracing::info!(target: "syncplay", player = name.as_str(), "sound started");
        let result = loop {
            let (h, payload) = match proto::read(&mut reader, || self.now()).await {
                Ok(m) => m,
                Err(e) => break Err(e),
            };
            match h.kind {
                proto::CODEC_HEADER => match proto::read_codec_header(&payload) {
                    Some(f) => stream.codec(id, f, h.received_us),
                    None => tracing::warn!(target: "syncplay", player = name.as_str(), "sound not PCM"),
                },
                proto::WIRE_CHUNK => {
                    if let Some(audio) = proto::read_wire_chunk(&payload) {
                        stream.push(id, audio, h.received_us);
                    }
                }
                proto::BREAK => stream.brk(id, h.received_us),
                _ => {}
            }
        };
        tracing::info!(target: "syncplay", player = name.as_str(), "sound stopped");
        stream.brk(id, self.now());
        result
    }

    /// A device's snapclient: the time exchange, its settings, and the sound of its group.
    async fn client(
        self: Arc<Self>,
        name: String,
        hello: proto::Header,
        mut reader: tokio::net::tcp::OwnedReadHalf,
        mut w: tokio::net::tcp::OwnedWriteHalf,
    ) -> std::io::Result<()> {
        tracing::info!(target: "syncplay", player = name.as_str(), "listener connected");
        // Read on its own (a read cut off halfway would lose the connection's place): the time requests come through here.
        let (asked_tx, mut asked) = mpsc::channel::<proto::Header>(64);
        let clock = self.clone();
        let reading = tokio::spawn(async move {
            while let Ok((h, _)) = proto::read(&mut reader, || clock.now()).await {
                if h.kind == proto::TIME && asked_tx.send(h).await.is_err() {
                    return;
                }
            }
        });
        let mut leader = self.groups.leader_of(&name);
        let mut volume = self.groups.volume_of(&name);
        let mut latency = self.groups.latency_of(&name);
        let mut stream = leader.as_deref().map(|l| self.stream(l));
        let mut sound = stream.as_ref().map(|s| s.out.subscribe());
        let mut muted = stream.as_ref().map(|s| s.muted.subscribe());
        let silent = |m: &Option<watch::Receiver<bool>>| m.as_ref().is_none_or(|m| *m.borrow());
        let settings = |volume: u8, latency: i32, muted: bool| proto::server_settings(self.buffer_ms, latency, volume, muted);
        let (mut said_muted, mut said_volume, mut said_latency) = (silent(&muted), volume, latency);
        w.write_all(&proto::message(proto::SERVER_SETTINGS, 0, hello.id, self.now(), &settings(volume, latency, said_muted))).await?;
        if let Some(c) = stream.as_ref().and_then(|s| s.state().codec.clone()) {
            w.write_all(&proto::message(proto::CODEC_HEADER, 0, 0, self.now(), &c)).await?;
        }
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        let result = loop {
            tokio::select! {
                h = asked.recv() => {
                    let Some(h) = h else { break Ok(()) };
                    let latency = proto::time(h.received_us - h.sent_us);
                    w.write_all(&proto::message(proto::TIME, h.id, h.id, self.now(), &latency)).await?;
                }
                o = next(&mut sound) => match o {
                    Ok(Out::Codec(c)) => w.write_all(&proto::message(proto::CODEC_HEADER, 0, 0, self.now(), &c)).await?,
                    Ok(Out::Chunk(c)) => w.write_all(&proto::message(proto::WIRE_CHUNK, 0, 0, self.now(), &c)).await?,
                    Err(broadcast::error::RecvError::Lagged(chunks)) => tracing::warn!(target: "syncplay", player = name.as_str(), chunks, "listener fell behind"),
                    Err(broadcast::error::RecvError::Closed) => sound = None,
                },
                _ = changed(&mut muted) => {}
                _ = tick.tick() => {
                    // Taken into another group, or out of one: its sound from there on.
                    let now_leader = self.groups.leader_of(&name);
                    if now_leader != leader {
                        leader = now_leader;
                        stream = leader.as_deref().map(|l| self.stream(l));
                        sound = stream.as_ref().map(|s| s.out.subscribe());
                        muted = stream.as_ref().map(|s| s.muted.subscribe());
                        if let Some(c) = stream.as_ref().and_then(|s| s.state().codec.clone()) {
                            w.write_all(&proto::message(proto::CODEC_HEADER, 0, 0, self.now(), &c)).await?;
                        }
                    }
                    volume = self.groups.volume_of(&name);
                    latency = self.groups.latency_of(&name);
                }
            }
            // Its settings when they changed: silenced or heard again, its volume, its speaker's delay.
            let now_muted = silent(&muted);
            if now_muted != said_muted || volume != said_volume || latency != said_latency {
                (said_muted, said_volume, said_latency) = (now_muted, volume, latency);
                w.write_all(&proto::message(proto::SERVER_SETTINGS, 0, 0, self.now(), &settings(volume, latency, now_muted))).await?;
            }
        };
        reading.abort();
        tracing::info!(target: "syncplay", player = name.as_str(), "listener disconnected");
        result
    }
}

/// The next of a group's sound; never, with no group.
async fn next(sound: &mut Option<broadcast::Receiver<Out>>) -> Result<Out, broadcast::error::RecvError> {
    match sound {
        Some(s) => s.recv().await,
        None => std::future::pending().await,
    }
}

/// The group's silence changed; never, with no group.
async fn changed(muted: &mut Option<watch::Receiver<bool>>) {
    match muted {
        Some(m) => {
            if m.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        }
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const F: Format = Format { rate: 50_000, bits: 16, channels: 2 };
    /// 20 ms of F.
    const CHUNK: usize = 50_000 * 4 / 50;

    /// Only a player with its secret listens or leads; anyone else's connection is closed at its hello.
    #[tokio::test]
    async fn sync_play_admits_only_a_player_with_its_secret() {
        struct Kept;
        impl Groups for Kept {
            fn leader_of(&self, _: &str) -> Option<String> {
                Some("lead".into())
            }
            fn volume_of(&self, _: &str) -> u8 {
                100
            }
            fn latency_of(&self, _: &str) -> i32 {
                0
            }
            fn admits(&self, player: &str, secret: &str) -> bool {
                (player, secret) == ("p1", "s1") || (player, secret) == ("lead", "s0")
            }
        }
        assert_eq!(named("s1:Kitchen: left"), ("s1", "Kitchen: left"));
        assert_eq!(named("p1"), ("", "p1"), "an app from before: no secret");
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(Snapcast::new(Arc::new(Kept), 1000).serve(listener));
        let hello = |id: &str| proto::message(proto::HELLO, 0, 0, 0, &proto::json(&serde_json::json!({ "ID": id })));
        // What the server answers first, or nothing (closed).
        async fn first(port: u16, m: Vec<u8>) -> usize {
            use tokio::io::AsyncReadExt;
            let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            s.write_all(&m).await.unwrap();
            let mut buf = [0u8; 64];
            tokio::time::timeout(Duration::from_secs(2), s.read(&mut buf)).await.unwrap().unwrap_or(0)
        }
        assert!(first(port, hello("s1:p1")).await > 0, "its settings come");
        assert_eq!(first(port, hello("p1")).await, 0, "without its secret: closed");
        assert_eq!(first(port, hello("s0:p1")).await, 0, "another's secret");
    }

    fn stamps(out: &[Out]) -> Vec<i64> {
        out.iter()
            .filter_map(|o| match o {
                Out::Chunk(c) => Some(i64::from(i32::from_le_bytes(c[0..4].try_into().unwrap())) * 1_000_000 + i64::from(i32::from_le_bytes(c[4..8].try_into().unwrap()))),
                Out::Codec(_) => None,
            })
            .collect()
    }

    /// With a real snapclient connecting to port 17040 as `p1` meanwhile (`cargo test -- --ignored real_snapclient`):
    /// 16 s of a 440 Hz tone sent as a leader's app sends it (half a second ahead, then as it plays), broken off for a
    /// second at 8 s.
    #[tokio::test]
    #[ignore]
    async fn real_snapclient() {
        struct One;
        impl Groups for One {
            fn leader_of(&self, _: &str) -> Option<String> {
                Some("lead".into())
            }
            fn volume_of(&self, _: &str) -> u8 {
                100
            }
            fn latency_of(&self, _: &str) -> i32 {
                0
            }
            fn admits(&self, _: &str, _: &str) -> bool {
                true
            }
        }
        let snap = Snapcast::new(Arc::new(One), 1000);
        tokio::spawn(snap.clone().serve(TcpListener::bind("0.0.0.0:17040").await.unwrap()));
        let mut src = TcpStream::connect("127.0.0.1:17040").await.unwrap();
        let msg = |kind, payload: &[u8]| proto::message(kind, 0, 0, 0, payload);
        src.write_all(&msg(proto::SOURCE, &proto::json(&serde_json::json!({ "player": "lead" })))).await.unwrap();
        let f = Format { rate: 44_100, bits: 16, channels: 2 };
        src.write_all(&msg(proto::CODEC_HEADER, &proto::codec_header_pcm(f))).await.unwrap();
        let start = Instant::now();
        let (mut sent, mut broke, mut paused_for) = (0u64, false, Duration::ZERO);
        while start.elapsed() < Duration::from_secs(16) {
            let t = start.elapsed();
            if !broke && t >= Duration::from_secs(8) {
                src.write_all(&msg(proto::BREAK, &[])).await.unwrap();
                broke = true;
                tokio::time::sleep(Duration::from_secs(1)).await;
                paused_for = Duration::from_secs(1);
                continue;
            }
            let due = ((t - paused_for).as_secs_f64() + 0.5) * 44_100.0;
            while (sent as f64) < due {
                let audio: Vec<u8> = (0..882u64)
                    .flat_map(|i| {
                        let v = ((((sent + i) as f64) * 440.0 * std::f64::consts::TAU / 44_100.0).sin() * 8000.0) as i16;
                        [v.to_le_bytes(), v.to_le_bytes()].concat()
                    })
                    .collect();
                src.write_all(&msg(proto::WIRE_CHUNK, &proto::wire_chunk(0, &audio))).await.unwrap();
                sent += 882;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    #[test]
    fn sound_is_stamped_as_it_follows_on_and_goes_out_at_its_stamp() {
        let s = Stream::new(1000);
        s.take_over(1, 0);
        s.release(0);
        s.codec(1, F, 1_000_000);
        // A burst at the start (the leader filling its own buffer), then in time.
        for _ in 0..5 {
            s.push(1, &[0; CHUNK], 1_000_000);
        }
        s.push(1, &[0; CHUNK], 1_090_000);
        let out = s.release(1_000_000);
        assert!(matches!(out[0], Out::Codec(_)));
        assert_eq!(stamps(&out), vec![1_000_000]);
        assert_eq!(stamps(&s.release(1_100_000)), vec![1_020_000, 1_040_000, 1_060_000, 1_080_000, 1_100_000]);
        // Another source (an old connection) is not heard.
        s.push(2, &[0; CHUNK], 1_100_000);
        assert!(s.state().held.is_empty());
    }

    #[test]
    fn a_break_silences_at_once_and_the_new_sound_is_heard_as_it_plays() {
        let s = Stream::new(1000);
        s.take_over(1, 0);
        s.codec(1, F, 0);
        // The first sound is heard as it plays (silent before: there is nothing to hear).
        s.push(1, &[0; CHUNK], 9_000_000);
        s.release(9_990_000);
        assert!(!*s.muted.borrow());
        for _ in 0..10 {
            s.push(1, &[0; CHUNK], 10_000_000);
        }
        s.release(10_050_000);
        s.brk(1, 10_050_000);
        assert!(*s.muted.borrow());
        assert!(s.state().held.is_empty(), "what was still to come goes");
        // Sound again 300 ms later: stamped as it comes, heard a buffer later (before its first chunk plays).
        s.push(1, &[0; CHUNK], 10_350_000);
        assert_eq!(stamps(&s.release(10_350_000)), vec![10_350_000]);
        s.release(11_300_000);
        assert!(*s.muted.borrow());
        s.release(11_330_000);
        assert!(!*s.muted.borrow());
    }

    /// A group winds with its leader (decision 0074): the leader's track winds down, its tape takes over, and the track
    /// comes back, sent as one sound with no break between. Every chunk follows on from the one before, the group never
    /// silenced, whether the tape sounds within the stall window or later.
    #[test]
    fn a_wind_hands_over_without_a_break() {
        let s = Stream::new(1000);
        s.take_over(1, 0);
        s.codec(1, F, 0);
        // The track plays (a burst filling the leader's output first, then in time).
        for i in 0..25 {
            s.push(1, &[0; CHUNK], 10_000_000 + (i / 5) * 20_000);
        }
        // Its quiet stop is over; the tape sounds 300 ms later (its stream asked for and ready): laid on end to end.
        for i in 0..50 {
            s.push(1, &[0; CHUNK], 10_400_000 + i * 20_000);
        }
        // The brake over, the track comes back 400 ms later, still in time.
        for i in 0..25 {
            s.push(1, &[0; CHUNK], 11_800_000 + i * 20_000);
        }
        let out = s.release(20_000_000);
        let st = stamps(&out);
        assert_eq!(st.len(), 100);
        assert!(st.windows(2).all(|w| w[1] - w[0] == 20_000), "one sound, chunk after chunk: {st:?}");
        assert!(!*s.muted.borrow(), "a wind is not a break");
        // A tape slow to sound (a cold stream): stamped as it comes, the group not silenced meanwhile.
        s.push(1, &[0; CHUNK], 30_000_000);
        assert_eq!(stamps(&s.release(30_000_000)), vec![30_000_000]);
        assert!(!*s.muted.borrow());
    }

    #[test]
    fn a_stall_is_stamped_anew_and_a_new_format_waits_for_the_old_sound_to_play() {
        let s = Stream::new(1000);
        s.take_over(1, 0);
        s.codec(1, F, 0);
        s.push(1, &[0; CHUNK], 5_000_000);
        // Nothing for 2 s: the next sound would be late; stamped as it comes.
        s.push(1, &[0; CHUNK], 7_000_000);
        assert_eq!(stamps(&s.release(7_000_000)), vec![5_000_000, 7_000_000]);
        // 48 kHz from here: the old sound ends at 7.02 s and plays out a buffer later; the header and the new sound after.
        let g = Format { rate: 48_000, ..F };
        s.codec(1, g, 7_005_000);
        s.push(1, &[0; 3840], 7_006_000);
        assert!(s.release(8_000_000).is_empty());
        let out = s.release(8_030_000);
        assert!(matches!(out[0], Out::Codec(_)));
        assert_eq!(stamps(&out), vec![8_030_000]);
    }
}
