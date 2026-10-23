//! Sync play for a player without Snapcast's player: the group's sound as one long WAV over HTTP for a Kodi
//! (`app::kodi`), as stamped records for a page in a browser (`Snapcast::frames`).
//!
//! - Each chunk is sent a buffer after its stamp, less the player's own delay (its speaker delay setting), so the
//!   player's own buffering brings it to its sound when the others play it.
//! - Where the group has no sound (a pause, a break, a stall), silence: the player never runs dry, and keeps its place.
//! - Kept in line from what the player says it plays (its time in the stream): the player's driver asks for a skip
//!   (it plays late) or a pad (early) of a few milliseconds, taken out of what is sent next, as snapclient drops and
//!   repeats samples.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::{broadcast, mpsc};

use super::proto::{self, Format};
use super::{Out, Snapcast};

/// Out of line by more than this (µs), a player is put back in step.
const OUT_OF_LINE_US: i64 = 20_000;
/// At most one correction this often: the next readings see the last one's effect.
const CORRECT_EVERY: Duration = Duration::from_secs(4);

/// One player listening.
#[derive(Default)]
pub struct Listener {
    /// The stamp (µs) of the sound at the stream's time 0, as moved by skips and pads: what the player plays at its
    /// time `t` in the stream was stamped `origin + t`. Zero until the first sound went out.
    origin: AtomicI64,
    /// Asked of what is sent next: skip (+) or pad with silence (−) this much (µs).
    adjust: AtomicI64,
    /// How late it played at its last readings (µs), and when it was last put back in step.
    lags: Mutex<(Vec<i64>, Option<Instant>)>,
    /// This minute's readings (µs late) and corrections, and when the minute began: how much it drifts, written down
    /// once a minute ([Drift]) before choosing how to correct it (the owner, 2026-10-23).
    minute: Mutex<(Vec<i64>, u32, Option<Instant>)>,
}

/// A minute of a player's readings: how late it played (ms; − early), the middle, the least and the most, how many
/// readings, and how many times it was put back in step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drift {
    pub median_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub readings: usize,
    pub corrections: u32,
}

/// The minute after which a minute's readings are written down.
const DRIFT_EVERY: Duration = Duration::from_secs(60);

impl Listener {
    /// The stamp (µs) of what the player plays at its time `at_ms` in the stream; `None` before anything was sent.
    pub fn stamp_at(&self, at_ms: f64) -> Option<i64> {
        let o = self.origin.load(Ordering::Relaxed);
        (o != 0).then(|| o + (at_ms * 1000.0) as i64)
    }

    /// Skip (+) or pad (−) `us` of what is sent next.
    pub fn adjust(&self, us: i64) {
        self.adjust.fetch_add(us, Ordering::Relaxed);
    }

    /// What the player played at its time `at_ms` in the stream, at the moment `when` (µs, Snapcast's clock), against
    /// what the others play then (a buffer after its stamp): the middle of five such readings, when out of line, put
    /// right with a skip or a pad. `Some((ms, late))` when it was.
    pub fn heard(&self, at_ms: f64, when: i64, buffer_ms: u32) -> Option<(i64, bool)> {
        let lag = when - i64::from(buffer_ms) * 1000 - self.stamp_at(at_ms)?;
        self.minute.lock().unwrap_or_else(|p| p.into_inner()).0.push(lag);
        let put = self.correct(lag);
        if put.is_some() {
            self.minute.lock().unwrap_or_else(|p| p.into_inner()).1 += 1;
        }
        put
    }

    /// The minute's readings, once a minute has passed since the first: then a new minute begins.
    pub fn drift(&self) -> Option<Drift> {
        let mut m = self.minute.lock().unwrap_or_else(|p| p.into_inner());
        let began = *m.2.get_or_insert_with(Instant::now);
        if began.elapsed() < DRIFT_EVERY || m.0.is_empty() {
            return None;
        }
        let mut lags = std::mem::take(&mut m.0);
        let corrections = std::mem::take(&mut m.1);
        m.2 = Some(Instant::now());
        lags.sort_unstable();
        let ms = |us: i64| (us as f64 / 100.0).round() / 10.0;
        Some(Drift { median_ms: ms(lags[lags.len() / 2]), min_ms: ms(lags[0]), max_ms: ms(lags[lags.len() - 1]), readings: lags.len(), corrections })
    }

    fn correct(&self, lag: i64) -> Option<(i64, bool)> {
        let mut held = self.lags.lock().unwrap_or_else(|p| p.into_inner());
        let (lags, corrected) = &mut *held;
        lags.push(lag);
        if lags.len() < 5 || corrected.is_some_and(|c| c.elapsed() < CORRECT_EVERY) {
            return None;
        }
        lags.sort_unstable();
        let middle = lags[lags.len() / 2];
        lags.clear();
        if middle.abs() <= OUT_OF_LINE_US {
            return None;
        }
        *corrected = Some(Instant::now());
        drop(held);
        self.adjust(middle);
        Some((middle.abs() / 1000, middle > 0))
    }
}

/// A WAV header for a stream of unknown length.
fn wav_header(f: Format) -> Vec<u8> {
    let mut h = Vec::with_capacity(44);
    h.extend_from_slice(b"RIFF");
    h.extend_from_slice(&u32::MAX.to_le_bytes());
    h.extend_from_slice(b"WAVE");
    h.extend_from_slice(b"fmt ");
    h.extend_from_slice(&16u32.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes());
    h.extend_from_slice(&f.channels.to_le_bytes());
    h.extend_from_slice(&f.rate.to_le_bytes());
    h.extend_from_slice(&(f.rate * u32::from(f.bits) * u32::from(f.channels) / 8).to_le_bytes());
    h.extend_from_slice(&(f.channels * f.bits.div_ceil(8)).to_le_bytes());
    h.extend_from_slice(&f.bits.to_le_bytes());
    h.extend_from_slice(b"data");
    h.extend_from_slice(&(u32::MAX - 36).to_le_bytes());
    h
}

/// The sound sent so far and still to send, in frames of the stamps' timeline (µs × rate).
struct Timeline {
    f: Format,
    /// (first frame, its audio), in order.
    chunks: VecDeque<(i64, Vec<u8>)>,
    /// The frame sent up to; `None` before the first.
    sent: Option<i64>,
}

impl Timeline {
    fn frame(&self, us: i64) -> i64 {
        (i128::from(us) * i128::from(self.f.rate) / 1_000_000) as i64
    }

    fn us(&self, frame: i64) -> i64 {
        (i128::from(frame) * 1_000_000 / i128::from(self.f.rate.max(1))) as i64
    }

    fn push(&mut self, stamp_us: i64, audio: Vec<u8>) {
        let at = self.frame(stamp_us);
        self.chunks.push_back((at, audio));
    }

    /// The audio due up to `to` (a frame), from where it was sent up to: the chunks' sound, silence between them.
    fn take(&mut self, to: i64, out: &mut Vec<u8>) {
        let fb = self.f.frame_bytes().max(1) as i64;
        let Some(mut at) = self.sent else { return };
        while at < to {
            // What lies wholly before `at` is past.
            while self.chunks.front().is_some_and(|(s, a)| s + a.len() as i64 / fb <= at) {
                self.chunks.pop_front();
            }
            match self.chunks.front() {
                Some((s, a)) if *s <= at => {
                    let from = ((at - s) * fb) as usize;
                    let end = (((to - s) * fb) as usize).min(a.len());
                    out.extend_from_slice(&a[from..end]);
                    at = s + end as i64 / fb;
                }
                next => {
                    let until = next.map_or(to, |(s, _)| (*s).min(to));
                    out.resize(out.len() + ((until - at) * fb) as usize, 0);
                    at = until;
                }
            }
        }
        self.sent = Some(at);
    }
}

impl Snapcast {
    /// The listener of player `name`, while it listens.
    pub fn listener(&self, name: &str) -> Option<Arc<Listener>> {
        self.listeners.lock().unwrap_or_else(|p| p.into_inner()).get(name).cloned()
    }

    /// The group's silence (a break: the others are silenced at once) to follow; `None` without such a group.
    pub fn muted_of(&self, leader: &str) -> Option<tokio::sync::watch::Receiver<bool>> {
        self.streams.lock().unwrap_or_else(|p| p.into_inner()).get(leader).map(|s| s.muted.subscribe())
    }

    /// The sound of `leader`'s group for player `name` (its speaker delay `delay_ms`), as WAV: the body of a response,
    /// which ends when the player goes away or the format changes (the player opens it again).
    pub fn listen(self: &Arc<Self>, leader: &str, name: &str, delay_ms: u32) -> mpsc::Receiver<Result<axum::body::Bytes, std::io::Error>> {
        let (tx, rx) = mpsc::channel(8);
        let (me, stream, name) = (self.clone(), self.stream(leader), name.to_string());
        let listener = Arc::new(Listener::default());
        self.listeners.lock().unwrap_or_else(|p| p.into_inner()).insert(name.clone(), listener.clone());
        let lag = i64::from(self.buffer_ms.saturating_sub(delay_ms)) * 1000;
        tokio::spawn(async move {
            let mut out = stream.out.subscribe();
            let mut format = stream.state().codec.clone().and_then(|c| proto::read_codec_header(&c));
            while format.is_none() {
                match out.recv().await {
                    Ok(Out::Codec(c)) => format = proto::read_codec_header(&c),
                    Ok(Out::Chunk(_)) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
            let f = format.expect("waited for");
            if tx.send(Ok(wav_header(f).into())).await.is_err() {
                return;
            }
            let mut line = Timeline { f, chunks: VecDeque::new(), sent: None };
            let mut tick = tokio::time::interval(Duration::from_millis(20));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    o = out.recv() => match o {
                        Ok(Out::Chunk(c)) => {
                            let stamp = i64::from(i32::from_le_bytes(c[0..4].try_into().unwrap_or_default())) * 1_000_000
                                + i64::from(i32::from_le_bytes(c[4..8].try_into().unwrap_or_default()));
                            if let Some(audio) = proto::read_wire_chunk(&c) {
                                line.push(stamp, audio.to_vec());
                            }
                        }
                        // Another format: this WAV ends; the player opens the group's sound again.
                        Ok(Out::Codec(c)) if proto::read_codec_header(&c) != Some(f) => break,
                        Ok(Out::Codec(_)) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                        Err(broadcast::error::RecvError::Closed) => break,
                    },
                    _ = tick.tick() => {
                        let due = line.frame(me.now() - lag);
                        let mut audio = Vec::new();
                        if line.sent.is_none() {
                            line.sent = Some(due);
                            listener.origin.store(line.us(due), Ordering::Relaxed);
                        }
                        // A skip moves on without sending; a pad sends silence without moving on.
                        let adjust = listener.adjust.swap(0, Ordering::Relaxed);
                        if adjust != 0 {
                            let frames = line.frame(adjust.abs());
                            if adjust > 0 {
                                line.sent = line.sent.map(|s| s + frames);
                            } else {
                                audio.resize(frames as usize * f.frame_bytes(), 0);
                            }
                            listener.origin.fetch_add(line.us(frames) * adjust.signum(), Ordering::Relaxed);
                        }
                        line.take(due, &mut audio);
                        if !audio.is_empty() && tx.send(Ok(audio.into())).await.is_err() {
                            break;
                        }
                    }
                }
            }
            let mut listeners = me.listeners.lock().unwrap_or_else(|p| p.into_inner());
            if listeners.get(&name).is_some_and(|l| Arc::ptr_eq(l, &listener)) {
                listeners.remove(&name);
            }
        });
        rx
    }
}

/// One record of a page's stream (`Snapcast::frames`): its kind, its stamp (Cratarr's time, epoch µs), its PCM.
fn record(kind: u8, stamp_us: i64, audio: &[u8]) -> axum::body::Bytes {
    let mut r = Vec::with_capacity(13 + audio.len());
    r.push(kind);
    r.extend_from_slice(&stamp_us.to_le_bytes());
    r.extend_from_slice(&(audio.len() as u32).to_le_bytes());
    r.extend_from_slice(audio);
    r.into()
}

/// Cratarr's time now (epoch µs) less its steady clock's: a stamp on the one, on the other.
fn to_epoch(snap: &Snapcast) -> i64 {
    let epoch = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_micros() as i64).unwrap_or(0);
    epoch - snap.now()
}

impl Snapcast {
    /// The sound of `leader`'s group for a page in a browser (decision 0037, amended 2026-10-23): each chunk as it comes,
    /// stamped on Cratarr's time (epoch µs), for the page to play at its moment on its own audio clock (a browser's
    /// `<audio>` holds seconds before it plays, more than the group's buffer). No pacing, no skips, no pads. First its
    /// format, 16 bytes: rate (u32), channels (u16), bits (u16), the group's buffer (ms, u32), the player's speaker delay
    /// (ms, u32). Then records: kind (u8: 1 sound, 2 silence from its stamp on: a break), stamp (i64), length (u32), the
    /// PCM. Little-endian.
    pub fn frames(self: &Arc<Self>, leader: &str, name: &str, delay_ms: u32) -> mpsc::Receiver<Result<axum::body::Bytes, std::io::Error>> {
        let (tx, rx) = mpsc::channel(64);
        let (me, stream, name, leader) = (self.clone(), self.stream(leader), name.to_string(), leader.to_string());
        tokio::spawn(async move {
            let mut out = stream.out.subscribe();
            let mut muted = stream.muted.subscribe();
            let mut format = stream.state().codec.clone().and_then(|c| proto::read_codec_header(&c));
            while format.is_none() {
                match out.recv().await {
                    Ok(Out::Codec(c)) => format = proto::read_codec_header(&c),
                    Ok(Out::Chunk(_)) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
            let f = format.expect("waited for");
            let mut head = Vec::with_capacity(16);
            head.extend_from_slice(&f.rate.to_le_bytes());
            head.extend_from_slice(&f.channels.to_le_bytes());
            head.extend_from_slice(&f.bits.to_le_bytes());
            head.extend_from_slice(&me.buffer_ms.to_le_bytes());
            head.extend_from_slice(&delay_ms.to_le_bytes());
            if tx.send(Ok(head.into())).await.is_err() {
                return;
            }
            tracing::info!(target: "syncplay", player = name.as_str(), leader = leader.as_str(), "page listening");
            let mut behind = 0u64;
            let why = loop {
                tokio::select! {
                    o = out.recv() => match o {
                        Ok(Out::Chunk(c)) => {
                            let stamp = i64::from(i32::from_le_bytes(c[0..4].try_into().unwrap_or_default())) * 1_000_000
                                + i64::from(i32::from_le_bytes(c[4..8].try_into().unwrap_or_default()));
                            let Some(audio) = proto::read_wire_chunk(&c) else { continue };
                            if tx.send(Ok(record(1, stamp + to_epoch(&me), audio))).await.is_err() {
                                break "gone";
                            }
                        }
                        Ok(Out::Codec(c)) if proto::read_codec_header(&c) != Some(f) => break "another format",
                        Ok(Out::Codec(_)) => {}
                        // The page reads too slowly: what it missed is lost to it (a gap it logs).
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            behind += n;
                            tracing::warn!(target: "syncplay", player = name.as_str(), chunks = n, "page fell behind");
                        }
                        Err(broadcast::error::RecvError::Closed) => break "group ended",
                    },
                    r = muted.changed() => {
                        if r.is_err() {
                            break "group ended";
                        }
                        let now = *muted.borrow_and_update();
                        if now && tx.send(Ok(record(2, me.now() + to_epoch(&me), &[]))).await.is_err() {
                            break "gone";
                        }
                    }
                }
            };
            tracing::info!(target: "syncplay", player = name.as_str(), leader = leader.as_str(), why, chunks_missed = behind, "page stopped listening");
        });
        rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const F: Format = Format { rate: 1000, bits: 16, channels: 1 };

    #[test]
    fn the_timeline_sends_the_chunks_and_silence_between_them() {
        let mut t = Timeline { f: F, chunks: VecDeque::new(), sent: Some(1_000) };
        // 1000 frames per second, 2 bytes a frame: a chunk of 10 frames at frame 1 005, another at 1 020.
        t.push(1_005_000, vec![1; 20]);
        t.push(1_020_000, vec![2; 20]);
        let mut out = Vec::new();
        t.take(1_012, &mut out);
        assert_eq!(out, [vec![0; 10], vec![1; 14]].concat(), "5 frames of silence, then 7 of the first chunk");
        out.clear();
        t.take(1_030, &mut out);
        assert_eq!(out, [vec![1; 6], vec![0; 10], vec![2; 20]].concat());
        assert_eq!(t.sent, Some(1_030));
        // What is past when it comes is not sent.
        t.push(1_000_000, vec![9; 20]);
        out.clear();
        t.take(1_035, &mut out);
        assert_eq!(out, vec![0; 10]);
    }

    #[test]
    fn a_listener_says_what_plays_at_its_time() {
        let l = Listener::default();
        assert_eq!(l.stamp_at(10.0), None);
        l.origin.store(5_000_000, Ordering::Relaxed);
        assert_eq!(l.stamp_at(1_500.5), Some(6_500_500));
        assert_eq!(&wav_header(F)[0..4], b"RIFF");
    }

    #[test]
    fn a_listener_is_put_back_in_step_from_the_middle_of_five_readings() {
        let l = Listener::default();
        l.origin.store(5_000_000, Ordering::Relaxed);
        // Played at its time 0 ms when the others played stamp 5 s + 30 ms (a 1 s buffer): 30 ms late, four times; once
        // a wild reading, which the middle leaves out.
        for when in [6_030_000, 6_030_000, 9_000_000, 6_030_000] {
            assert_eq!(l.heard(0.0, when, 1000), None);
        }
        assert_eq!(l.heard(0.0, 6_030_000, 1000), Some((30, true)));
        assert_eq!(l.adjust.load(Ordering::Relaxed), 30_000, "a skip of 30 ms asked for");
        // In line (10 ms): left as it is.
        let l = Listener::default();
        l.origin.store(5_000_000, Ordering::Relaxed);
        for _ in 0..5 {
            assert_eq!(l.heard(0.0, 6_010_000, 1000), None);
        }
        assert_eq!(l.adjust.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_minute_of_readings_is_written_down_as_its_middle_least_and_most() {
        let l = Listener::default();
        l.origin.store(5_000_000, Ordering::Relaxed);
        assert_eq!(l.drift(), None, "a minute begins");
        for when in [6_001_000, 6_003_500, 6_002_000] {
            l.heard(0.0, when, 1000);
        }
        assert_eq!(l.drift(), None, "not a minute yet");
        l.minute.lock().unwrap().2 = Instant::now().checked_sub(Duration::from_secs(61));
        assert_eq!(l.drift(), Some(Drift { median_ms: 2.0, min_ms: 1.0, max_ms: 3.5, readings: 3, corrections: 0 }));
        assert_eq!(l.drift(), None, "a new minute");
    }

    #[test]
    fn a_page_s_record_is_its_kind_stamp_length_and_sound() {
        let r = record(1, -2, &[7, 8, 9]);
        assert_eq!(r[0], 1);
        assert_eq!(i64::from_le_bytes(r[1..9].try_into().unwrap()), -2);
        assert_eq!(u32::from_le_bytes(r[9..13].try_into().unwrap()), 3);
        assert_eq!(&r[13..], &[7, 8, 9]);
    }
}
