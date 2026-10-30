//! What the single-track intake's gate hears in a file's sound (0.10, the owner; decision 0080), in the one decode the
//! gate makes already (`acoustid::print`), mixed to mono:
//! - **Where its highs stop.** A Hann-windowed FFT of every [`FFT`] samples, its power averaged over the track, then
//!   the highest brick wall in it: [`WALL_DB`] lower in the 750 Hz just above an edge than in the 750 Hz just below it
//!   (centres 1.25 kHz apart), and nothing above coming back. A lossy encoder's low pass leaves one (LAME: about
//!   16 kHz at 128 kbit/s, 19 at 192, 20 at 256 and 320); a natural roll-off, however low or quiet, does not. Only an
//!   edge from [`LOWEST_EDGE`] to [`HIGHEST_EDGE`], and [`NYQUIST_MARGIN`] or more below the file's Nyquist, counts.
//!   A file sold as lossless with one is not lossless; an MP3 whose highs stop [`REENCODED_KHZ`] below its bitrate's
//!   own low pass was made from a lower one. (Not `spectrum`'s band bank: it stops at 16 kHz and cannot place an edge
//!   to a bin.)
//! - **Clicks and pops**, by the nightly library scanner's rule, so the two agree (its calibration on the library:
//!   Essentia's ClickDetector, frames of 512 every 256). Here a click is a sudden jump from one sample to the next (the
//!   second difference) in a hop of [`HOP`]; it is a defect when the peak at it (a hop either side) is more than
//!   [`RATIO_DB`] above the RMS of the 200 ms before it (ending a hop before it) and that 200 ms is below
//!   [`SILENT_DB`]: a spike out of digital silence. A musical transient sits 2 to 33 dB above loud context and never
//!   counts; raw click counts mean nothing (electronic music has dozens of harmless ones). Not in the first
//!   [`SKIP_SECS`] (a track starting after leading silence looks just like a pop), and only a spike, not the start of a
//!   sound: the 200 ms from [`QUIET_AGAIN_SECS`] after it are quiet again, unless the file ends first. One in the last
//!   [`TAIL_SECS`] is a pop at the very end (Cri D'Amour's: its last 32 ms at full scale after 2.8 s near -90 dB, 92 to
//!   94 dB above it).
//! - **Clipping**: a flat run of [`CLIP_RUN`] or more samples at full scale (within 0.1 dB of it, and within one 16-bit
//!   step of each other) in any channel, more than [`CLIPS_AT_MOST`] times. A loud master's odd full-scale sample, a
//!   rounded crest near it, or a lossy decode's overshoot past it (never flat), passes.
//!
//! One pass over the sound; per hop four numbers are kept (about 2.8 KB a second at 44.1 kHz), the clicks judged at
//! the end.

use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};
use symphonia::core::codecs::audio::AudioCodecId;

/// The spectrum's frame (samples).
pub const FFT: usize = 2048;
/// A brick wall: this much lower (dB) just above an edge than just below it.
const WALL_DB: f64 = 30.0;
/// And everything above the edge at least this much below the band just below it (dB): the sound does not come back.
const STAYS_DOWN_DB: f64 = 20.0;
/// The edges that count (Hz): from below a 112 kbit/s MP3's low pass to a 256's.
const LOWEST_EDGE: f64 = 14_000.0;
const HIGHEST_EDGE: f64 = 20_200.0;
/// An edge counts only this far below the file's Nyquist frequency or more (Hz): what a converter's anti-alias filter
/// leaves just below it is not a lossy encoder's.
const NYQUIST_MARGIN: f64 = 1_500.0;
/// No edge is looked for above this (Hz): what a hi-res file holds above hearing is not judged.
const SEARCHED_TO: f64 = 24_000.0;
/// An MP3 was made from a lower bitrate when its highs stop this far below its own bitrate's low pass (kHz).
const REENCODED_KHZ: f64 = 2.0;

/// Clicks are looked for a hop at a time (samples).
const HOP: usize = 256;
/// The context before a click (s).
const CONTEXT_SECS: f64 = 0.2;
/// A defect: the peak this far above the context's RMS (dB), the context below `SILENT_DB` (dBFS).
const RATIO_DB: f64 = 60.0;
const SILENT_DB: f64 = -55.0;
/// A sudden jump: the hop's sharpest step at least this many times the steps' RMS before it (20 dB).
const JUMP: f64 = 10.0;
/// Nothing in the first second counts.
const SKIP_SECS: f64 = 1.0;
/// A spike within this of the end is a pop at the very end (s).
const TAIL_SECS: f64 = 3.0;
/// After a spike, the sound is quiet again from this long after it (s); spikes closer than it are one.
const QUIET_AGAIN_SECS: f64 = 0.05;
/// The most times a refusal lists.
const LISTED: usize = 3;

/// Full scale, less 0.1 dB.
const FULL: f32 = 0.988_553;
/// A flat run's samples are within this of its first (one 16-bit step).
const FLAT: f32 = 1.0 / 32_768.0;
/// Samples in a row that make a clip, and the most clips that pass.
const CLIP_RUN: u32 = 3;
const CLIPS_AT_MOST: u32 = 3;
/// Clips closer than this (s) are one (both channels at once, say).
const SAME_CLIP_SECS: f64 = 0.01;

/// Below any level that matters, so silence has a decibel figure.
const TINY: f64 = 1e-20;

/// What a file is sold as: lossless, an MP3 at its bitrate (its audio's bytes over its length), or anything else (not
/// judged by its highs).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Sold {
    Lossless,
    Mp3 {
        kbps: u32,
    },
    #[default]
    Other,
}

impl Sold {
    pub fn of(codec: AudioCodecId, bytes: u64, secs: f64) -> Sold {
        use symphonia::core::codecs::audio::well_known::*;
        let pcm = (CODEC_ID_PCM_S32LE..=CODEC_ID_PCM_F64BE_PLANAR).contains(&codec);
        if pcm || [CODEC_ID_FLAC, CODEC_ID_ALAC, CODEC_ID_WAVPACK, CODEC_ID_MONKEYS_AUDIO, CODEC_ID_TTA].contains(&codec) {
            Sold::Lossless
        } else if codec == CODEC_ID_MP3 && secs > 0.0 {
            Sold::Mp3 { kbps: (bytes as f64 * 8.0 / secs / 1000.0).round() as u32 }
        } else {
            Sold::Other
        }
    }
}

/// The low pass LAME gives a bitrate (kHz).
fn lame_low_pass(kbps: u32) -> f64 {
    match kbps {
        0..=128 => 16.0,
        129..=160 => 17.5,
        161..=192 => 19.0,
        _ => 20.0,
    }
}

/// What was heard.
#[derive(Debug, Clone, Default)]
pub struct Findings {
    pub sold: Sold,
    /// Where a brick wall stops its highs (Hz).
    pub cutoff_hz: Option<f64>,
    /// When each click is (s), the end's left out.
    pub clicks: Vec<f64>,
    pub pop_at_end: bool,
    /// Flat runs at full scale, and when the first is (s).
    pub clips: u32,
    pub first_clip: Option<f64>,
}

impl Findings {
    /// Why the gate refuses the file, when it does: its highs first, then clicks, a pop at the end and clipping.
    pub fn why(&self) -> Option<String> {
        if let Some(hz) = self.cutoff_hz {
            let khz = hz / 1000.0;
            match self.sold {
                Sold::Lossless => return Some(format!("Not lossless: it stops at {}", khz_text(khz))),
                Sold::Mp3 { kbps } if khz < lame_low_pass(kbps) - REENCODED_KHZ => {
                    return Some(format!("Re-encoded from a lower bitrate: it stops at {}", khz_text(khz)));
                }
                _ => {}
            }
        }
        let mut parts = Vec::new();
        match self.clicks.len() {
            0 => {}
            1 => parts.push(format!("a click at {}", clock(self.clicks[0]))),
            n => {
                let times = self.clicks.iter().take(LISTED).map(|&t| clock(t)).collect::<Vec<_>>().join(", ");
                parts.push(format!("{n} clicks at {times}{}", if n > LISTED { ", …" } else { "" }));
            }
        }
        if self.pop_at_end {
            parts.push("a pop at the very end".to_string());
        }
        if self.clips > CLIPS_AT_MOST
            && let Some(t) = self.first_clip
        {
            parts.push(format!("clipped at {}", clock(t)));
        }
        let text = parts.join("; ");
        let mut chars = text.chars();
        chars.next().map(|c| c.to_uppercase().chain(chars).collect())
    }
}

/// kHz to the nearest half: "16 kHz", "17.5 kHz".
fn khz_text(khz: f64) -> String {
    let r = (khz * 2.0).round() / 2.0;
    if r.fract() == 0.0 { format!("{r:.0} kHz") } else { format!("{r:.1} kHz") }
}

/// A time as m:ss.
fn clock(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// One hop's levels (mono).
#[derive(Debug, Clone, Copy)]
struct Level {
    /// Mean square, and the largest sample.
    ms: f32,
    peak: f32,
    /// The second difference's mean square and largest size: how suddenly the sound moves, sample to sample.
    jump_ms: f32,
    jump_peak: f32,
}

#[derive(Default)]
struct HopSum {
    sq: f64,
    peak: f32,
    jump_sq: f64,
    jump_peak: f32,
    n: usize,
}

/// Hears a file's sound a packet at a time ([`Flaws::hear`]), then tells what it found ([`Flaws::finish`]).
pub struct Flaws {
    rate: u32,
    /// Frames heard so far.
    heard: u64,
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    frame: Vec<f32>,
    input: Vec<f32>,
    output: Vec<Complex<f32>>,
    /// Each bin's power, summed over the frames.
    power: Vec<f64>,
    frames: u32,
    hop: HopSum,
    hops: Vec<Level>,
    /// The two mono samples before, for the second difference.
    before: [f32; 2],
    /// Per channel: the flat run at full scale it is in (its length and first value).
    runs: Vec<(u32, f32)>,
    clips: u32,
    first_clip: Option<u64>,
    last_clip: Option<u64>,
}

impl Flaws {
    pub fn new(rate: u32) -> Self {
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(FFT);
        let window = (0..FFT).map(|i| (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / FFT as f64).cos()) as f32).collect();
        let (input, output) = (fft.make_input_vec(), fft.make_output_vec());
        Flaws {
            rate,
            heard: 0,
            power: vec![0.0; output.len()],
            fft,
            window,
            frame: Vec::with_capacity(FFT),
            input,
            output,
            frames: 0,
            hop: HopSum::default(),
            hops: Vec::new(),
            before: [0.0; 2],
            runs: Vec::new(),
            clips: 0,
            first_clip: None,
            last_clip: None,
        }
    }

    /// One packet's sound, planar, -1..1 (a lossy decode's may overshoot it).
    pub fn hear(&mut self, planes: &[Vec<f32>]) {
        let Some(n) = planes.first().map(Vec::len) else { return };
        if self.runs.len() != planes.len() {
            self.runs = vec![(0, 0.0); planes.len()];
        }
        let same_clip = (SAME_CLIP_SECS * f64::from(self.rate)) as u64;
        for i in 0..n {
            let at = self.heard + i as u64;
            let mut sum = 0.0f32;
            for (c, p) in planes.iter().enumerate() {
                let x = p[i];
                sum += x;
                let run = &mut self.runs[c];
                if x.abs() < FULL {
                    run.0 = 0;
                } else if run.0 > 0 && (x - run.1).abs() <= FLAT {
                    run.0 += 1;
                } else {
                    *run = (1, x);
                }
                if run.0 == CLIP_RUN && self.last_clip.is_none_or(|l| at > l + same_clip) {
                    self.clips += 1;
                    self.first_clip = self.first_clip.or(Some(at));
                    self.last_clip = Some(at);
                }
            }
            self.listen(sum / planes.len() as f32);
        }
        self.heard += n as u64;
    }

    /// One mono sample: into the spectrum's frame and the hop.
    fn listen(&mut self, x: f32) {
        self.frame.push(x);
        if self.frame.len() == FFT {
            for ((i, &s), &w) in self.input.iter_mut().zip(&self.frame).zip(&self.window) {
                *i = s * w;
            }
            if self.fft.process(&mut self.input, &mut self.output).is_ok() {
                for (p, c) in self.power.iter_mut().zip(&self.output) {
                    *p += f64::from(c.norm_sqr());
                }
                self.frames += 1;
            }
            self.frame.clear();
        }
        let jump = x - 2.0 * self.before[1] + self.before[0];
        self.before = [self.before[1], x];
        let h = &mut self.hop;
        h.sq += f64::from(x) * f64::from(x);
        h.peak = h.peak.max(x.abs());
        h.jump_sq += f64::from(jump) * f64::from(jump);
        h.jump_peak = h.jump_peak.max(jump.abs());
        h.n += 1;
        if h.n == HOP {
            self.close_hop();
        }
    }

    fn close_hop(&mut self) {
        let h = std::mem::take(&mut self.hop);
        if h.n == 0 {
            return;
        }
        let n = h.n as f64;
        self.hops.push(Level { ms: (h.sq / n) as f32, peak: h.peak, jump_ms: (h.jump_sq / n) as f32, jump_peak: h.jump_peak });
    }

    /// What was heard, the file sold as `sold`.
    pub fn finish(mut self, sold: Sold) -> Findings {
        self.close_hop();
        let rate = f64::from(self.rate);
        let secs = self.heard as f64 / rate;
        let cutoff_hz = if self.frames > 0 {
            let frames = f64::from(self.frames);
            let db: Vec<f64> = self.power.iter().map(|p| 10.0 * (p / frames + TINY).log10()).collect();
            wall(&db, rate)
        } else {
            None
        };
        let (pops, clicks): (Vec<f64>, Vec<f64>) = spikes(&self.hops, rate).into_iter().partition(|&t| t >= secs - TAIL_SECS);
        Findings { sold, cutoff_hz, clicks, pop_at_end: !pops.is_empty(), clips: self.clips, first_clip: self.first_clip.map(|at| at as f64 / rate) }
    }
}

/// The highest brick wall in an averaged spectrum (dB per bin, [`FFT`] / 2 + 1 of them): its edge (Hz), when it is
/// one that counts.
fn wall(db: &[f64], rate: f64) -> Option<f64> {
    let bin = rate / FFT as f64;
    let nyquist = rate / 2.0;
    let to = (nyquist - 250.0).min(SEARCHED_TO);
    // The mean level (dB) of the bins from `lo` to `hi` Hz.
    let mean = |lo: f64, hi: f64| -> Option<f64> {
        let a = (lo / bin).ceil().max(0.0) as usize;
        let b = ((hi / bin).floor().max(0.0) as usize).min(db.len() - 1);
        (a <= b).then(|| db[a..=b].iter().sum::<f64>() / (b - a + 1) as f64)
    };
    // An edge at `f` Hz: how far it drops, when it is a wall.
    let drop_at = |f: f64| -> Option<f64> {
        let below = mean(f - 1000.0, f - 250.0)?;
        let above = mean(f + 250.0, f + 1000.0)?;
        let rest = mean(f + 250.0, to)?;
        (below - above >= WALL_DB && below - rest >= STAYS_DOWN_DB).then_some(below - above)
    };
    // From the top down: the highest run of walls, and where in it the drop is deepest.
    let top = ((to - 1000.0) / bin).floor().max(0.0) as usize;
    let bottom = ((LOWEST_EDGE - 1000.0) / bin).ceil() as usize;
    let mut best: Option<(f64, f64)> = None;
    for k in (bottom..=top).rev() {
        let f = k as f64 * bin;
        match drop_at(f) {
            Some(d) => {
                if best.is_none_or(|(b, _)| d > b) {
                    best = Some((d, f));
                }
            }
            None if best.is_some() => break,
            None => {}
        }
    }
    let (_, f) = best?;
    // The edge itself: the highest bin near it whose level (with two bins either side) is still above half way down.
    let half = (mean(f - 1000.0, f - 250.0)? + mean(f + 250.0, f + 1000.0)?) / 2.0;
    let lo = ((f - 1000.0) / bin).ceil().max(2.0) as usize;
    let hi = (((f + 1000.0) / bin).floor() as usize).min(db.len() - 3);
    let edge = (lo..=hi).rev().find(|&k| db[k - 2..=k + 2].iter().sum::<f64>() / 5.0 >= half)? as f64 * bin;
    (edge >= LOWEST_EDGE && edge <= HIGHEST_EDGE.min(nyquist - NYQUIST_MARGIN)).then_some(edge)
}

fn mean_of(hops: &[Level], f: impl Fn(&Level) -> f32) -> f64 {
    hops.iter().map(|h| f64::from(f(h))).sum::<f64>() / hops.len().max(1) as f64
}

fn db_of(ms: f64) -> f64 {
    10.0 * (ms + TINY).log10()
}

/// When each click or pop is (s), by the nightly scanner's rule: see the module.
fn spikes(hops: &[Level], rate: f64) -> Vec<f64> {
    let per_sec = rate / HOP as f64;
    let context = ((CONTEXT_SECS * per_sec).round() as usize).max(1);
    let gap = ((QUIET_AGAIN_SECS * per_sec).ceil() as usize).max(1);
    let skip = (SKIP_SECS * per_sec).ceil() as usize;
    let mut out = Vec::new();
    let mut last: Option<usize> = None;
    for j in skip.max(2)..hops.len() {
        // The 200 ms before, ending a hop before this one.
        let before = &hops[j.saturating_sub(1 + context)..j - 1];
        if f64::from(hops[j].jump_peak) <= JUMP * mean_of(before, |h| h.jump_ms).sqrt() + 1e-9 {
            continue;
        }
        let peak = hops[j - 1..(j + 2).min(hops.len())].iter().fold(0.0f32, |m, h| m.max(h.peak));
        let quiet = db_of(mean_of(before, |h| h.ms));
        if quiet >= SILENT_DB || 20.0 * (f64::from(peak) + 1e-10).log10() - quiet <= RATIO_DB {
            continue;
        }
        // A spike, not the start of a sound: quiet again after it, unless the file ends first.
        if let Some(after) = hops.get(j + gap..j + gap + context)
            && db_of(mean_of(after, |h| h.ms)) >= SILENT_DB
        {
            continue;
        }
        if last.is_some_and(|l| j - l <= gap) {
            continue;
        }
        last = Some(j);
        out.push(j as f64 / per_sec);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wall_is_found_to_a_bin_and_only_where_it_counts() {
        let bin = 44_100.0 / FFT as f64;
        let spectrum = |level: &dyn Fn(f64) -> f64| (0..=FFT / 2).map(|k| level(k as f64 * bin)).collect::<Vec<f64>>();
        // A lossy encoder's: everything to 19 kHz, nothing above.
        let edge = wall(&spectrum(&|hz: f64| if hz <= 19_000.0 { -40.0 } else { -140.0 }), 44_100.0).unwrap();
        assert!((edge - 19_000.0).abs() < 50.0, "{edge}");
        // A converter's anti-alias filter at 21 kHz: too near Nyquist to count.
        assert_eq!(wall(&spectrum(&|hz: f64| if hz <= 21_000.0 { -40.0 } else { -140.0 }), 44_100.0), None);
        // A steep but natural roll-off (24 dB an octave from 2 kHz): no wall.
        assert_eq!(wall(&spectrum(&|hz: f64| -40.0 - 24.0 * (hz.max(2_000.0) / 2_000.0).log2()), 44_100.0), None);
        // A notch that comes back up is no wall.
        assert_eq!(wall(&spectrum(&|hz: f64| if (16_000.0..16_900.0).contains(&hz) { -140.0 } else { -40.0 }), 44_100.0), None);
    }

    #[test]
    fn an_mp3_is_judged_against_its_own_bitrates_low_pass() {
        let why = |kbps: u32, hz: f64| Findings { sold: Sold::Mp3 { kbps }, cutoff_hz: Some(hz), ..Default::default() }.why();
        assert_eq!(why(320, 16_000.0).as_deref(), Some("Re-encoded from a lower bitrate: it stops at 16 kHz"));
        assert_eq!(why(320, 17_480.0).as_deref(), Some("Re-encoded from a lower bitrate: it stops at 17.5 kHz"));
        assert_eq!(why(192, 16_000.0).as_deref(), Some("Re-encoded from a lower bitrate: it stops at 16 kHz"));
        assert_eq!(why(320, 19_000.0), None, "a 192's low pass in a 320: near enough");
        assert_eq!(why(128, 16_000.0), None, "a 128 is what it says it is");
        let other = Findings { sold: Sold::Other, cutoff_hz: Some(16_000.0), ..Default::default() };
        assert_eq!(other.why(), None, "AAC and the rest are not judged by their highs");
    }

    #[test]
    fn finds_are_told_with_their_times_three_at_most() {
        let f = Findings { clicks: vec![83.2, 185.9, 200.0, 250.0], pop_at_end: true, clips: 4, first_clip: Some(161.5), ..Default::default() };
        assert_eq!(f.why().as_deref(), Some("4 clicks at 1:23, 3:05, 3:20, …; a pop at the very end; clipped at 2:41"));
        let one = Findings { clicks: vec![83.2], ..Default::default() };
        assert_eq!(one.why().as_deref(), Some("A click at 1:23"));
        let few = Findings { clips: CLIPS_AT_MOST, first_clip: Some(161.5), ..Default::default() };
        assert_eq!(few.why(), None, "a few clips pass");
        assert_eq!(Findings { sold: Sold::Lossless, ..Default::default() }.why(), None);
    }
}
