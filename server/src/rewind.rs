//! Tape rewind (0.9.9): a track heard backwards from a moment to its start, as a tape rewinds, rendered here as one clip
//! that both players play as it is, then the track from its start. Here, not in the players: they do not keep the sound
//! they played (the page keeps 30 s behind), and one renderer serves both.
//!
//! The speed follows an eased curve: spinning up over the first quarter, flat, braking over the last quarter, its area
//! the distance rewound, so the clip lasts exactly as asked and ends exactly at 0:00. Faster than the sound's own rate
//! the treble would fold back down (aliasing): each moment is read from a pyramid of half-band levels, each half the rate
//! of the one below, at the level its speed needs (two blended, so a change of level is not heard), built only as far as
//! the block's fastest moment needs. Rendered in blocks from the moment backwards: the first, a short window before the
//! moment, is decoded and sent first, so the clip starts at once.

use std::f64::consts::PI;

use anyhow::Result;

/// The clip's length is held to this (milliseconds): the players ask for 4 × the start/stop ramp, at most 12 s.
const SHORTEST_MS: u64 = 200;
const LONGEST_MS: u64 = 12_000;
/// Spinning up, and braking, each take this share of the clip.
const EASE: f64 = 0.25;
/// The first block (seconds of the clip): sent at once.
const FIRST: f64 = 0.1;
/// Each later block.
const BLOCK: f64 = 0.25;
/// At most this much of the track is decoded for one block (seconds), however fast it rewinds there.
const SPAN: f64 = 8.0;
/// The pyramid's levels above the sound itself: up to 4096 × its speed.
const LEVELS: usize = 12;
/// The half-band filter's reach each side (63 taps).
const HALF: usize = 31;
/// Decoded before a block's own room (frames): an MP3's first frame after a seek may come out wrong.
const PREROLL: f64 = 4096.0;
/// Faded in and out (seconds): no click where the track stops and the clip starts.
const FADE: f64 = 0.005;

/// What a clip is rendered from: a file (`analysis::Reading`), or in the tests a tone.
pub trait Sound {
    /// The sound from about `from` on (planar, -1..1), until at least `to` unless it ends first; with the frame it
    /// starts at (at or before `from`).
    fn read(&mut self, from: u64, to: u64) -> Result<(u64, Vec<Vec<f32>>)>;
}

impl Sound for crate::analysis::Reading {
    fn read(&mut self, from: u64, to: u64) -> Result<(u64, Vec<Vec<f32>>)> {
        let start = self.seek(from)?;
        let mut planes = vec![Vec::new(); self.channels()];
        let (mut at, mut packet) = (start, Vec::new());
        while at < to && self.next(&mut packet)? {
            at += packet[0].len() as u64;
            for (p, s) in planes.iter_mut().zip(&packet) {
                p.extend_from_slice(s);
            }
        }
        Ok((start, planes))
    }
}

/// One clip: from where, how long, in what format.
pub struct Rewind {
    /// The sound's rate, and the clip's: the sound's up to 48 kHz, else 44.1 or 48 kHz as its family.
    rate: u32,
    out_rate: u32,
    /// The clip's channels: the sound's first two.
    channels: usize,
    /// Where it starts (frames of the sound).
    from: f64,
    /// Its length (frames of the clip).
    frames: usize,
}

impl Rewind {
    pub fn new(rate: u32, channels: usize, from_ms: u64, ms: u64) -> Self {
        let out_rate = if rate <= 48_000 {
            rate
        } else if rate % 44_100 == 0 {
            44_100
        } else {
            48_000
        };
        let ms = ms.clamp(SHORTEST_MS, LONGEST_MS);
        Self {
            rate,
            out_rate,
            channels: channels.clamp(1, 2),
            from: from_ms as f64 * f64::from(rate) / 1000.0,
            frames: (ms * u64::from(out_rate) / 1000) as usize,
        }
    }

    /// The whole WAV's length (bytes).
    pub fn bytes(&self) -> u64 {
        44 + self.data()
    }

    fn data(&self) -> u64 {
        self.frames as u64 * self.channels as u64 * 2
    }

    fn header(&self) -> Vec<u8> {
        let (data, ch) = (self.data() as u32, self.channels as u16);
        let mut h = Vec::with_capacity(44);
        h.extend_from_slice(b"RIFF");
        h.extend_from_slice(&(36 + data).to_le_bytes());
        h.extend_from_slice(b"WAVEfmt ");
        h.extend_from_slice(&16u32.to_le_bytes());
        h.extend_from_slice(&1u16.to_le_bytes());
        h.extend_from_slice(&ch.to_le_bytes());
        h.extend_from_slice(&self.out_rate.to_le_bytes());
        h.extend_from_slice(&(self.out_rate * u32::from(ch) * 2).to_le_bytes());
        h.extend_from_slice(&(ch * 2).to_le_bytes());
        h.extend_from_slice(&16u16.to_le_bytes());
        h.extend_from_slice(b"data");
        h.extend_from_slice(&data.to_le_bytes());
        h
    }

    /// The curve's height at clip frame `i` (0..1); `scale` makes its area the distance.
    fn shape(&self, i: usize) -> f64 {
        let u = (i as f64 + 0.5) / self.frames as f64;
        let e = u.min(1.0 - u) / EASE;
        if e >= 1.0 { 1.0 } else { (1.0 - (PI * e).cos()) / 2.0 }
    }

    /// The WAV, header first, a block at a time to `send`; one that answers false (the player stopped reading) ends it.
    pub fn render(&self, sound: &mut dyn Sound, mut send: impl FnMut(Vec<u8>) -> bool) -> Result<()> {
        if !send(self.header()) {
            return Ok(());
        }
        let total: f64 = (0..self.frames).map(|i| self.shape(i)).sum();
        let scale = if total > 0.0 { self.from / total } else { 0.0 };
        let fade = ((FADE * f64::from(self.out_rate)) as usize).max(1);
        let span = SPAN * f64::from(self.rate);
        let mut dither = crate::convert::Tpdf::new();
        let (mut i, mut gone) = (0, 0.0);
        while i < self.frames {
            // Where each frame of the block reads the sound, and how fast it moves there (frames of the sound a frame).
            let most = (((if i == 0 { FIRST } else { BLOCK }) * f64::from(self.out_rate)) as usize).max(1);
            let (start, here) = (i, self.from - gone);
            let (mut at, mut speed) = (Vec::with_capacity(most), Vec::with_capacity(most));
            while i < self.frames && i - start < most && here - (self.from - gone) < span {
                let s = self.shape(i) * scale;
                at.push(self.from - gone);
                speed.push(s);
                gone += s;
                i += 1;
            }
            let block = self.block(sound, &at, &speed)?;
            let mut bytes = Vec::with_capacity(at.len() * self.channels * 2);
            for j in 0..at.len() {
                let edge = (start + j).min(self.frames - 1 - (start + j));
                let gain = if edge < fade { (1.0 - (PI * (edge as f64 + 0.5) / fade as f64).cos()) / 2.0 } else { 1.0 };
                for c in &block {
                    let v = f64::from(c[j]) * gain * 32767.0 + dither.next();
                    bytes.extend_from_slice(&(v.round().clamp(-32768.0, 32767.0) as i16).to_le_bytes());
                }
            }
            if !send(bytes) {
                return Ok(());
            }
        }
        Ok(())
    }

    /// One block (planar, the clip's channels): the sound under it decoded with room for the filters, its levels built
    /// as far as its fastest frame needs, each frame read at its own.
    fn block(&self, sound: &mut dyn Sound, at: &[f64], speed: &[f64]) -> Result<Vec<Vec<f32>>> {
        // A speed's place in the pyramid: level ⌊λ⌋, blended with the next by its fraction.
        let level = |s: f64| s.max(1.0).log2().min(LEVELS as f64);
        let top = speed.iter().map(|&s| level(s).ceil() as usize).max().unwrap_or(0);
        let room = (HALF + 4) as f64 * f64::from(1u32 << top);
        let lo = at.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = at.iter().copied().fold(0.0, f64::max);
        let (first, planes) = sound.read((lo - room - PREROLL).max(0.0) as u64, (hi + room).ceil() as u64)?;
        let mut levels = vec![Level { base: first as i64, planes: planes.into_iter().take(self.channels).collect() }];
        while levels.len() <= top {
            let half = levels[levels.len() - 1].half();
            levels.push(half);
        }
        let mut out = vec![Vec::with_capacity(at.len()); levels[0].planes.len()];
        for (&p, &s) in at.iter().zip(speed) {
            let l = level(s);
            let k = l.floor() as usize;
            let g = l - k as f64;
            for (c, o) in out.iter_mut().enumerate() {
                let mut v = levels[k].at(c, p / f64::from(1u32 << k));
                if g > 0.0 {
                    v = v * (1.0 - g) + levels[k + 1].at(c, p / f64::from(1u32 << (k + 1))) * g;
                }
                o.push(v as f32);
            }
        }
        // A file with fewer channels than its header said: the clip's channels still.
        while out.len() < self.channels {
            out.push(out.first().cloned().unwrap_or_else(|| vec![0.0; at.len()]));
        }
        Ok(out)
    }
}

/// One level of the pyramid: sample `j` stands at frame `j × 2^level` of the sound; `base` is the first one's `j`.
struct Level {
    base: i64,
    planes: Vec<Vec<f32>>,
}

impl Level {
    /// The level above: half-band filtered (zero phase, so it stays in place) and every other sample kept.
    fn half(&self) -> Level {
        let taps = half_band();
        let len = self.planes.first().map_or(0, Vec::len) as i64;
        let first = self.base.div_euclid(2) + i64::from(self.base.rem_euclid(2) != 0);
        let last = (self.base + len - 1).div_euclid(2);
        let n = (last - first + 1).max(0) as usize;
        let planes = self
            .planes
            .iter()
            .map(|x| {
                let s = |k: i64| if k >= 0 && k < len { x[k as usize] } else { 0.0 };
                (0..n as i64)
                    .map(|j| {
                        let c = 2 * (first + j) - self.base;
                        // Its even taps but the middle are zero.
                        let mut v = 0.5 * s(c);
                        for (o, t) in taps.iter().enumerate() {
                            let o = 2 * o as i64 + 1;
                            v += t * (s(c - o) + s(c + o));
                        }
                        v
                    })
                    .collect()
            })
            .collect();
        Level { base: first, planes }
    }

    /// Channel `c` at sample `q` (fractional), between its neighbours on a Catmull-Rom curve; silence outside.
    fn at(&self, c: usize, q: f64) -> f64 {
        let x = &self.planes[c];
        let i = q.floor();
        let f = q - i;
        let i = i as i64 - self.base;
        let s = |k: i64| if k >= 0 && (k as usize) < x.len() { f64::from(x[k as usize]) } else { 0.0 };
        let (a, b, c, d) = (s(i - 1), s(i), s(i + 1), s(i + 2));
        b + 0.5 * f * (c - a + f * (2.0 * a - 5.0 * b + 4.0 * c - d + f * (3.0 * (b - c) + d - a)))
    }
}

/// The half-band filter's odd taps, nearest first (Blackman-windowed sinc, summing to 1 with the middle's 0.5).
fn half_band() -> &'static [f32; (HALF + 1) / 2] {
    static TAPS: std::sync::OnceLock<[f32; (HALF + 1) / 2]> = std::sync::OnceLock::new();
    TAPS.get_or_init(|| {
        let mut t = [0.0f64; (HALF + 1) / 2];
        for (k, v) in t.iter_mut().enumerate() {
            let o = (2 * k + 1) as f64;
            let w = 0.42 + 0.5 * (PI * o / (HALF + 1) as f64).cos() + 0.08 * (2.0 * PI * o / (HALF + 1) as f64).cos();
            *v = (PI * o / 2.0).sin() / (PI * o) * w;
        }
        let sum: f64 = t.iter().sum();
        t.map(|v| (v * 0.25 / sum) as f32)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tone in both channels, `secs` long.
    struct Tone {
        hz: f64,
        secs: f64,
    }

    impl Sound for Tone {
        fn read(&mut self, from: u64, to: u64) -> Result<(u64, Vec<Vec<f32>>)> {
            let to = to.min((self.secs * 44_100.0) as u64);
            let one: Vec<f32> = (from..to).map(|i| (0.5 * (2.0 * PI * self.hz * i as f64 / 44_100.0).sin()) as f32).collect();
            Ok((from, vec![one.clone(), one]))
        }
    }

    /// The clip as samples of its first channel (-1..1), after its header.
    fn clip(r: &Rewind, sound: &mut dyn Sound) -> (Vec<u8>, Vec<f64>) {
        let mut wav = Vec::new();
        r.render(sound, |b| {
            wav.extend_from_slice(&b);
            true
        })
        .unwrap();
        let left = wav[44..].chunks_exact(4).map(|f| f64::from(i16::from_le_bytes([f[0], f[1]])) / 32768.0).collect();
        (wav, left)
    }

    fn rms(x: &[f64]) -> f64 {
        (x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64).sqrt()
    }

    #[test]
    fn the_clip_lasts_as_asked_and_its_curve_covers_the_whole_way_back() {
        let r = Rewind::new(44_100, 2, 30_000, 2_000);
        let (wav, _) = clip(&r, &mut Tone { hz: 440.0, secs: 60.0 });
        assert_eq!(wav.len() as u64, r.bytes());
        assert_eq!(r.bytes(), 44 + 2 * 44_100 * 2 * 2);
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[36..40], b"data");
        // Still at both ends, its area the distance.
        assert!(r.shape(0) < 1e-4 && r.shape(r.frames - 1) < 1e-4);
        assert_eq!(r.shape(r.frames / 2), 1.0);
        let area: f64 = (0..r.frames).map(|i| r.shape(i)).sum::<f64>() / r.frames as f64;
        assert!((area - (1.0 - EASE)).abs() < 1e-3, "{area}");
        // Asked for longer than the longest: the longest; and a 96 kHz file comes as 48 kHz.
        assert_eq!(Rewind::new(96_000, 2, 30_000, 60_000).bytes(), 44 + 12 * 48_000 * 2 * 2);
    }

    #[test]
    fn rewinding_fast_does_not_fold_the_treble_down() {
        // A minute back in 2 s: 40 × at full speed, so 15 kHz goes far past hearing and 40 Hz becomes 1.6 kHz.
        let r = Rewind::new(44_100, 2, 60_000, 2_000);
        let middle = |x: &[f64]| x[x.len() * 2 / 5..x.len() * 3 / 5].to_vec();
        let (_, treble) = clip(&r, &mut Tone { hz: 15_000.0, secs: 90.0 });
        let (_, bass) = clip(&r, &mut Tone { hz: 40.0, secs: 90.0 });
        assert!(rms(&middle(&treble)) < 0.005, "treble folded down: {}", rms(&middle(&treble)));
        assert!(rms(&middle(&bass)) > 0.3, "the bass lost: {}", rms(&middle(&bass)));
    }

    #[test]
    fn a_level_up_keeps_its_place_and_the_low_sound() {
        let x: Vec<f32> = (0..4000).map(|i| (2.0 * PI * 100.0 * f64::from(i) / 44_100.0).sin() as f32).collect();
        let l = Level { base: 1001, planes: vec![x.clone()] };
        let h = l.half();
        assert_eq!(h.base, 501);
        // Sample j of the level above stands where sample 2j stands below.
        for j in [600i64, 1000, 1400] {
            let below = f64::from(x[(2 * j - 1001) as usize]);
            assert!((h.at(0, j as f64) - below).abs() < 1e-3, "{j}: {} vs {below}", h.at(0, j as f64));
        }
    }
}
