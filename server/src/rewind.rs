//! Tape rewind (0.9.9): a track heard backwards from a moment to its start, as a tape rewinds, rendered here as one clip
//! that both players play as it is, then the track from its start. Here, not in the players: they do not keep the sound
//! they played (the page keeps 30 s behind), and one renderer serves both.
//!
//! The speed follows an eased curve (`Shape`, the owner's four controls since 0.9.10): spinning up, cruising at a middle
//! speed, braking, so the clip lasts exactly as asked and ends exactly at 0:00. Where the cruise would not get back in
//! time, it is cut: pieces of it heard, spread evenly over the track between, each jump crossfaded; where it would get
//! back sooner, it cruises slower, never faster than asked and never cut when it need not be. Faster than the sound's own rate
//! the treble would fold back down (aliasing): each moment is read from a pyramid of half-band levels, each half the rate
//! of the one below, at the level its speed needs (two blended, so a change of level is not heard), built only as far as
//! the block's fastest moment needs. Rendered in blocks from the moment backwards: the first, a short window before the
//! moment, is decoded and sent first, so the clip starts at once.

use std::f64::consts::PI;

use anyhow::Result;

/// The clip's length is held to this (milliseconds).
const SHORTEST_MS: u64 = 200;
const LONGEST_MS: u64 = 12_000;
/// A jump between two pieces, crossfaded over this long (seconds, equal power): no click.
pub(crate) const XFADE: f64 = 0.015;
/// The first block (seconds of the clip): sent at once.
pub(crate) const FIRST: f64 = 0.1;
/// Each later block.
pub(crate) const BLOCK: f64 = 0.25;
/// At most this much of the track is decoded for one block (seconds), however fast it rewinds there.
pub(crate) const SPAN: f64 = 8.0;
/// The pyramid's levels above the sound itself: up to 4096 × its speed.
const LEVELS: usize = 12;
/// The half-band filter's reach each side (63 taps).
const HALF: usize = 31;
/// Decoded before a block's own room (frames): an MP3's first frame after a seek may come out wrong.
const PREROLL: f64 = 4096.0;
/// Faded in and out (seconds): no click where the track stops and the clip starts.
pub(crate) const FADE: f64 = 0.005;

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

/// How a clip sounds (the owner, 2026-10-27: shorter, at a middle speed, pieces cut out; four controls): its length
/// (milliseconds), its cruising speed (× the track's), the length of each piece heard while cruising (milliseconds of
/// the clip), and the share of the clip spinning up takes, and braking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shape {
    pub ms: u64,
    pub speed: f64,
    pub piece_ms: u64,
    pub ramp: f64,
}

impl Default for Shape {
    fn default() -> Self {
        Self { ms: 2_500, speed: 6.0, piece_ms: 350, ramp: 0.2 }
    }
}

impl Shape {
    /// Held to what sounds like a tape (and to what a player of 0.9.9, asking 4 × its ramp, may ask).
    pub fn held(self) -> Self {
        Self {
            ms: self.ms.clamp(SHORTEST_MS, LONGEST_MS),
            speed: if self.speed.is_finite() { self.speed.clamp(2.0, 16.0) } else { 6.0 },
            piece_ms: self.piece_ms.clamp(100, 1_000),
            ramp: if self.ramp.is_finite() { self.ramp.clamp(0.05, 0.4) } else { 0.2 },
        }
    }
}

/// One clip: from where, how, in what format.
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
    shape: Shape,
    /// The cruising speed (frames of the track a frame of the clip), and whether the clip is cut (`cruise`).
    v: f64,
    cut: bool,
}

/// A stretch of the clip heard as one backward run of the track: its frames (with the crossfades' overlap), where the
/// track is at its first, and where it fades in and out (equal power) into the runs beside it.
#[derive(Debug, Clone, PartialEq)]
struct Run {
    from: usize,
    to: usize,
    top: f64,
    fade_in: Option<(usize, usize)>,
    fade_out: Option<(usize, usize)>,
}

impl Run {
    fn gain(&self, i: usize) -> f64 {
        let ease = |(a, b): (usize, usize), up: bool| {
            let t = ((i as f64 + 0.5 - a as f64) / (b - a) as f64).clamp(0.0, 1.0);
            if up { (t * PI / 2.0).sin() } else { (t * PI / 2.0).cos() }
        };
        self.fade_in.map_or(1.0, |w| if i < w.1 { ease(w, true) } else { 1.0 }) * self.fade_out.map_or(1.0, |w| if i >= w.0 { ease(w, false) } else { 1.0 })
    }
}

impl Rewind {
    pub fn new(rate: u32, channels: usize, from_ms: u64, shape: Shape) -> Self {
        let out_rate = if rate <= 48_000 {
            rate
        } else if rate % 44_100 == 0 {
            44_100
        } else {
            48_000
        };
        let shape = shape.held();
        let mut r = Self {
            rate,
            out_rate,
            channels: channels.clamp(1, 2),
            from: from_ms as f64 * f64::from(rate) / 1000.0,
            frames: (shape.ms * u64::from(out_rate) / 1000) as usize,
            shape,
            v: 0.0,
            cut: false,
        };
        (r.v, r.cut) = r.cruise();
        r
    }

    /// The whole WAV's length (bytes).
    pub fn bytes(&self) -> u64 {
        44 + self.data()
    }

    fn data(&self) -> u64 {
        self.frames as u64 * self.channels as u64 * 2
    }

    fn header(&self) -> Vec<u8> {
        wav_header(self.out_rate, self.channels, self.data())
    }

    /// Spinning up and braking (frames of the clip each).
    fn ramp(&self) -> usize {
        ((self.shape.ramp * self.frames as f64).round() as usize).clamp(1, self.frames / 2)
    }

    /// The curve's height at clip frame `i` (0..1): eased up, flat, eased down.
    fn height(&self, i: usize) -> f64 {
        let r = self.ramp() as f64;
        let e = (i as f64 + 0.5).min(self.frames as f64 - i as f64 - 0.5) / r;
        if e >= 1.0 { 1.0 } else { (1.0 - (PI * e.max(0.0)).cos()) / 2.0 }
    }

    /// The cruising speed (frames of the track a frame of the clip), and whether the clip is cut: at the speed asked
    /// it would not get back to the start in time. Not cut, the speed is lowered so that it gets there exactly.
    fn cruise(&self) -> (f64, bool) {
        let asked = self.shape.speed * f64::from(self.rate) / f64::from(self.out_rate);
        let area: f64 = (0..self.frames).map(|i| self.height(i)).sum();
        if area <= 0.0 {
            return (0.0, false);
        }
        if asked * area >= self.from { (self.from / area, false) } else { (asked, true) }
    }

    /// How fast the track goes at clip frame `i`.
    fn speed(&self, i: usize) -> f64 {
        self.v * self.height(i)
    }

    /// The runs the clip is made of: one, or (cut) the first spinning up into the first piece, the pieces spread evenly
    /// over the track between, the last braking into 0:00; each jump crossfaded.
    fn runs(&self) -> Vec<Run> {
        let (v, cut) = (self.v, self.cut);
        let n = self.frames;
        if !cut {
            return vec![Run { from: 0, to: n, top: self.from, fade_in: None, fade_out: None }];
        }
        let r = self.ramp();
        let cruise = n - 2 * r;
        let piece = (self.shape.piece_ms as f64 * f64::from(self.out_rate) / 1000.0).max(1.0);
        let k = ((cruise as f64 / piece).round() as usize).max(1);
        let edge = |i: usize| r + (i * cruise + k / 2) / k;
        // Where the track is when the cruise begins, and where it must be when the brake begins.
        let top = self.from - (0..r).map(|i| self.speed(i)).sum::<f64>();
        let bottom: f64 = (n - r..n).map(|i| self.speed(i)).sum();
        let covered = |a: usize, b: usize| v * (b - a) as f64;
        // Each piece's own top: the first's where the spin-up left the track, the last's so that it ends where the brake
        // begins (one piece: the brake its own run), the rest evenly between.
        let tops: Vec<f64> = if k == 1 {
            vec![top]
        } else {
            let last = bottom + covered(edge(k - 1), edge(k));
            (0..k).map(|i| top - (top - last) * i as f64 / (k - 1) as f64).collect()
        };
        let mut runs: Vec<(usize, usize, f64)> = (0..k).map(|i| (edge(i), edge(i + 1), tops[i])).collect();
        runs[0].0 = 0;
        runs[0].2 = self.from;
        if k == 1 {
            runs.push((n - r, n, bottom));
        } else {
            runs[k - 1].1 = n;
        }
        let half = ((XFADE * f64::from(self.out_rate) / 2.0) as usize).max(1);
        let last = runs.len() - 1;
        runs.iter()
            .enumerate()
            .map(|(j, &(a, b, top))| {
                let from = if j == 0 { a } else { a.saturating_sub(half) };
                // Before its own first frame it is where the track was a little later.
                let top = top + (from..a).map(|i| self.speed(i)).sum::<f64>();
                Run {
                    from,
                    to: if j == last { b } else { (b + half).min(n) },
                    top,
                    fade_in: (j > 0).then_some((from, a + half)),
                    fade_out: (j < last).then_some((b.saturating_sub(half), (b + half).min(n))),
                }
            })
            .collect()
    }

    /// The WAV, header first, a block at a time to `send`; one that answers false (the player stopped reading) ends it.
    pub fn render(&self, sound: &mut dyn Sound, mut send: impl FnMut(Vec<u8>) -> bool) -> Result<()> {
        if !send(self.header()) {
            return Ok(());
        }
        let runs = self.runs();
        let mut out = vec![vec![0f32; self.frames]; self.channels];
        let fade = ((FADE * f64::from(self.out_rate)) as usize).max(1);
        let span = SPAN * f64::from(self.rate);
        let mut dither = crate::convert::Tpdf::new();
        let mut sent = 0;
        for (j, run) in runs.iter().enumerate() {
            let (mut i, mut at) = (run.from, run.top);
            while i < run.to {
                // Where each frame of the block reads the track, and how fast it moves there.
                let most = (((if i == 0 { FIRST } else { BLOCK }) * f64::from(self.out_rate)) as usize).max(1);
                let (start, here) = (i, at);
                let (mut ats, mut speeds) = (Vec::with_capacity(most), Vec::with_capacity(most));
                while i < run.to && i - start < most && here - at < span {
                    let s = self.speed(i);
                    ats.push(at.max(0.0));
                    speeds.push(s);
                    at -= s;
                    i += 1;
                }
                let block = read_block(sound, self.channels, &ats, &speeds)?;
                for (c, b) in out.iter_mut().zip(&block) {
                    for (k, v) in b.iter().enumerate() {
                        c[start + k] += (f64::from(*v) * run.gain(start + k)) as f32;
                    }
                }
                // What no later run adds to is ready.
                let ready = runs.get(j + 1).map_or(i, |next| i.min(next.from));
                if ready > sent {
                    let mut bytes = Vec::with_capacity((ready - sent) * self.channels * 2);
                    for f in sent..ready {
                        let edge = f.min(self.frames - 1 - f);
                        let gain = if edge < fade { (1.0 - (PI * (edge as f64 + 0.5) / fade as f64).cos()) / 2.0 } else { 1.0 };
                        for c in &out {
                            let v = f64::from(c[f]) * gain * 32767.0 + dither.next();
                            bytes.extend_from_slice(&(v.round().clamp(-32768.0, 32767.0) as i16).to_le_bytes());
                        }
                    }
                    sent = ready;
                    if !send(bytes) {
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }
}

/// One block (planar, the clip's channels): the sound under it decoded with room for the filters, its levels built
/// as far as its fastest frame needs, each frame read at its own.
pub fn read_block(sound: &mut dyn Sound, channels: usize, at: &[f64], speed: &[f64]) -> Result<Vec<Vec<f32>>> {
    // A speed's place in the pyramid: level ⌊λ⌋, blended with the next by its fraction.
    let level = |s: f64| s.max(1.0).log2().min(LEVELS as f64);
    let top = speed.iter().map(|&s| level(s).ceil() as usize).max().unwrap_or(0);
    let room = (HALF + 4) as f64 * f64::from(1u32 << top);
    let lo = at.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = at.iter().copied().fold(0.0, f64::max);
    let (first, planes) = sound.read((lo - room - PREROLL).max(0.0) as u64, (hi + room).ceil() as u64)?;
    let mut levels = vec![Level { base: first as i64, planes: planes.into_iter().take(channels).collect() }];
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
    while out.len() < channels {
        out.push(out.first().cloned().unwrap_or_else(|| vec![0.0; at.len()]));
    }
    Ok(out)
}

/// A 16-bit PCM WAV's header for `data` bytes of sound.
pub fn wav_header(rate: u32, channels: usize, data: u64) -> Vec<u8> {
    let (data, ch) = (data as u32, channels as u16);
    let mut h = Vec::with_capacity(44);
    h.extend_from_slice(b"RIFF");
    h.extend_from_slice(&(36 + data).to_le_bytes());
    h.extend_from_slice(b"WAVEfmt ");
    h.extend_from_slice(&16u32.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes());
    h.extend_from_slice(&ch.to_le_bytes());
    h.extend_from_slice(&rate.to_le_bytes());
    h.extend_from_slice(&(rate * u32::from(ch) * 2).to_le_bytes());
    h.extend_from_slice(&(ch * 2).to_le_bytes());
    h.extend_from_slice(&16u16.to_le_bytes());
    h.extend_from_slice(b"data");
    h.extend_from_slice(&data.to_le_bytes());
    h
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

    #[test]
    fn a_rewinds_longest_and_shortest_are_the_tables() {
        let r: serde_json::Value = serde_json::from_str(include_str!("../../shared/rules.json")).unwrap();
        assert_eq!((r["wind_shortest_ms"].as_u64(), r["wind_cap_ms"].as_u64()), (Some(SHORTEST_MS), Some(LONGEST_MS)));
    }

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

    fn shaped(ms: u64, speed: f64) -> Shape {
        Shape { ms, speed, ..Shape::default() }
    }

    #[test]
    fn the_clip_lasts_as_asked() {
        let r = Rewind::new(44_100, 2, 30_000, shaped(2_000, 6.0));
        let (wav, _) = clip(&r, &mut Tone { hz: 440.0, secs: 60.0 });
        assert_eq!(wav.len() as u64, r.bytes());
        assert_eq!(r.bytes(), 44 + 2 * 44_100 * 2 * 2);
        assert_eq!((&wav[..4], &wav[36..40]), (&b"RIFF"[..], &b"data"[..]));
        // Asked for longer than the longest: the longest; and a 96 kHz file comes as 48 kHz.
        assert_eq!(Rewind::new(96_000, 2, 30_000, shaped(60_000, 6.0)).bytes(), 44 + 12 * 48_000 * 2 * 2);
    }

    #[test]
    fn a_short_way_back_is_not_cut_and_goes_slower_than_asked() {
        // 3 s back in 2.5 s at 6 × would overshoot: one run, about 1.5 ×, its area exactly the way back.
        let r = Rewind::new(44_100, 2, 3_000, Shape::default());
        assert!(!r.cut);
        assert_eq!(r.runs().len(), 1);
        assert!((r.v - 1.5).abs() < 0.01, "{}", r.v);
        let back: f64 = (0..r.frames).map(|i| r.speed(i)).sum();
        assert!((back - r.from).abs() < 1.0, "{back} of {}", r.from);
        assert!(r.speed(0) < 1e-3 && r.speed(r.frames - 1) < 1e-3, "still at both ends");
    }

    #[test]
    fn a_long_way_back_is_cut_into_pieces_spread_over_it() {
        // Three minutes back: at 6 × only 12 s are heard, in four pieces of about 350 ms.
        let r = Rewind::new(44_100, 2, 180_000, Shape::default());
        assert!(r.cut);
        let runs = r.runs();
        assert_eq!(runs.len(), 4);
        assert_eq!((runs[0].from, runs[0].top, runs[3].to), (0, r.from, r.frames));
        // The last run brakes into 0:00.
        let last = &runs[3];
        let end = last.top - (last.from..last.to).map(|i| r.speed(i)).sum::<f64>();
        assert!(end.abs() < 1.0, "ends at {end}");
        // The pieces stand evenly apart, going back.
        let gaps: Vec<f64> = runs.windows(2).skip(1).map(|w| w[0].top - w[1].top).collect();
        assert!(gaps.windows(2).all(|g| (g[0] - g[1]).abs() < 2.0), "{gaps:?}");
        // At a jump the two runs' gains keep the power.
        let (a, b) = (&runs[1], &runs[2]);
        for i in b.from..a.to {
            let p = a.gain(i).powi(2) + b.gain(i).powi(2);
            assert!((p - 1.0).abs() < 1e-6, "{i}: {p}");
        }
    }

    #[test]
    fn rewinding_fast_does_not_fold_the_treble_down() {
        // Two minutes back in 2 s at 16 ×: 15 kHz goes far past hearing, 40 Hz becomes 640 Hz.
        let r = Rewind::new(44_100, 2, 120_000, shaped(2_000, 16.0));
        let middle = |x: &[f64]| x[x.len() * 2 / 5..x.len() * 3 / 5].to_vec();
        let (_, treble) = clip(&r, &mut Tone { hz: 15_000.0, secs: 150.0 });
        let (_, bass) = clip(&r, &mut Tone { hz: 40.0, secs: 150.0 });
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
