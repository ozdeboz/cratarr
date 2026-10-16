//! Sync play for a player without Snapcast's player (a Kodi, `app::kodi`): the group's sound as one long WAV over HTTP.
//!
//! - Each chunk is sent a buffer after its stamp, less the player's own delay (its speaker delay setting), so the
//!   player's own buffering brings it to its sound when the others play it.
//! - Where the group has no sound (a pause, a break, a stall), silence: the player never runs dry, and keeps its place.
//! - Kept in line from what the player says it plays (its time in the stream): the player's driver asks for a skip
//!   (it plays late) or a pad (early) of a few milliseconds, taken out of what is sent next, as snapclient drops and
//!   repeats samples.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use tokio::sync::{broadcast, mpsc};

use super::proto::{self, Format};
use super::{Out, Snapcast};

/// One player listening.
#[derive(Default)]
pub struct Listener {
    /// The stamp (µs) of the sound at the stream's time 0, as moved by skips and pads: what the player plays at its
    /// time `t` in the stream was stamped `origin + t`. Zero until the first sound went out.
    origin: AtomicI64,
    /// Asked of what is sent next: skip (+) or pad with silence (−) this much (µs).
    adjust: AtomicI64,
}

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
}
