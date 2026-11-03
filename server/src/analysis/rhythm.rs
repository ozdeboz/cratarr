//! Rhythm (0.15, the owner, 2026-11-02): four on the floor as a score, and the time signature.
//!
//! - **Four on the floor:** a kick on every beat and few between. The kicks are the low band's hits (below about
//!   120 Hz: two low-pass filters, 24 dB an octave), picked from its loudness in hundredths of a second where it rises
//!   sharply against what came before. The score is the share of beats a kick lands on, within the stretches that have
//!   kicks at all (a breakdown does not count against it), times the share of kicks that land on a beat, times how much
//!   of the track has kicks (half or more counts whole). 0 to 1; `FOUR_ON_THE_FLOOR` and over is four on the floor.
//! - **The time signature:** from Beat this!'s downbeats (decision 0049, amended): the beats in each bar, the most
//!   common count, sure as the share of bars that have it. 4/4 and 3/4 come out sure; odd meters less so.

use serde::{Deserialize, Serialize};

/// A score at or over this is four on the floor.
pub const FOUR_ON_THE_FLOOR: f32 = 0.55;
/// The low band's top (Hz).
const LOW: f32 = 120.0;
/// Hundredths of a second.
const FRAMES_A_SECOND: u32 = 100;
/// A kick this close to a beat is on it (s).
const ON_BEAT: f64 = 0.06;
/// A rise in the low band's loudness of at least this much (dB) against the frame before is a start.
const RISE_DB: f32 = 4.0;
/// Two kicks closer than this are one (s): a kick rings longer than a hundredth.
const KICK_GAP: f64 = 0.1;
/// Beats a stretch: one with at least two kicks is a stretch with kicks.
const STRETCH: usize = 8;
/// At least this many beats to say anything.
const FEWEST_BEATS: usize = 16;
/// At least this many bars for a time signature, and this share agreeing.
const FEWEST_BARS: usize = 8;
const AGREEING: f32 = 0.6;

/// A track's rhythm as the analysis keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rhythm {
    /// Four on the floor, 0 to 1 (`FOUR_ON_THE_FLOOR` and over is).
    pub four: f32,
}

/// The low band's kicks, heard a sample at a time.
pub struct KickEar {
    filters: [Biquad; 2],
    hop: usize,
    in_frame: usize,
    energy: f32,
    frames: Vec<f32>,
}

impl KickEar {
    pub fn new(rate: u32) -> Self {
        let lp = Biquad::low_pass(rate as f32, LOW);
        Self { filters: [lp, lp], hop: (rate / FRAMES_A_SECOND).max(1) as usize, in_frame: 0, energy: 0.0, frames: Vec::new() }
    }

    /// One sample of the sound's middle.
    pub fn hear(&mut self, s: f32) {
        let low = self.filters[0].run(s);
        let y = self.filters[1].run(low);
        self.energy += y * y;
        self.in_frame += 1;
        if self.in_frame == self.hop {
            self.frames.push(self.energy / self.hop as f32);
            (self.energy, self.in_frame) = (0.0, 0);
        }
    }

    /// The kicks' times (s).
    pub fn kicks(&self) -> Vec<f64> {
        kicks_of(&self.frames)
    }
}

/// The kicks in the low band's loudness, a frame a hundredth of a second: a frame that rises `RISE_DB` over the one
/// before, loud enough to matter (a tenth of the band's loud frames' level and up), the strongest within `KICK_GAP`.
fn kicks_of(frames: &[f32]) -> Vec<f64> {
    if frames.len() < 3 {
        return Vec::new();
    }
    let db: Vec<f32> = frames.iter().map(|e| 10.0 * (e + 1e-12).log10()).collect();
    let mut sorted = db.clone();
    sorted.sort_by(f32::total_cmp);
    let loud = sorted[sorted.len() * 9 / 10];
    let floor = loud - 20.0;
    let mut out: Vec<(f64, f32)> = Vec::new();
    for k in 1..db.len() {
        let rise = db[k] - db[k - 1].max(floor - 10.0);
        if rise < RISE_DB || db[k] < floor {
            continue;
        }
        let t = k as f64 / f64::from(FRAMES_A_SECOND);
        match out.last_mut() {
            Some((last, r)) if t - *last < KICK_GAP => {
                if rise > *r {
                    (*last, *r) = (t, rise);
                }
            }
            _ => out.push((t, rise)),
        }
    }
    out.into_iter().map(|(t, _)| t).collect()
}

/// Four on the floor, 0 to 1, from the kicks and the beats (s, both in order); none with too few beats.
pub fn four_on_the_floor(kicks: &[f64], beats: &[f64]) -> Option<f32> {
    if beats.len() < FEWEST_BEATS {
        return None;
    }
    let near = |list: &[f64], t: f64| {
        let i = list.partition_point(|x| *x < t - ON_BEAT);
        list.get(i).is_some_and(|x| (x - t).abs() <= ON_BEAT)
    };
    let on: Vec<bool> = beats.iter().map(|b| near(kicks, *b)).collect();
    // The stretches with kicks: STRETCH beats from each beat on, at least two kicks among them.
    let mut active = vec![false; beats.len()];
    for start in (0..beats.len()).step_by(STRETCH / 2) {
        let end = (start + STRETCH).min(beats.len());
        let (from, to) = (beats[start] - ON_BEAT, beats[end - 1] + ON_BEAT);
        let n = kicks.partition_point(|k| *k < to) - kicks.partition_point(|k| *k < from);
        if n >= 2 {
            active[start..end].iter_mut().for_each(|a| *a = true);
        }
    }
    let actives = active.iter().filter(|a| **a).count();
    if actives == 0 || kicks.is_empty() {
        return Some(0.0);
    }
    let landed = on.iter().zip(&active).filter(|(o, a)| **o && **a).count() as f32 / actives as f32;
    let on_beat = kicks.iter().filter(|k| near(beats, **k)).count() as f32 / kicks.len() as f32;
    let covered = (actives as f32 / beats.len() as f32 / 0.5).min(1.0);
    Some((landed * on_beat * covered).clamp(0.0, 1.0))
}

/// The time signature from Beat this!'s beats and downbeats (s, in order): beats a bar and how sure (the share of bars
/// with that many). None with too few bars, or none agreeing enough.
pub fn meter(beats: &[f64], downbeats: &[f64]) -> Option<(u8, f32)> {
    let mut counts = std::collections::BTreeMap::<usize, usize>::new();
    let mut bars = 0;
    for w in downbeats.windows(2) {
        let n = beats.partition_point(|b| *b < w[1] - 0.015) - beats.partition_point(|b| *b < w[0] - 0.015);
        if (2..=12).contains(&n) {
            *counts.entry(n).or_default() += 1;
            bars += 1;
        }
    }
    if bars < FEWEST_BARS {
        return None;
    }
    let (n, c) = counts.into_iter().max_by_key(|(n, c)| (*c, std::cmp::Reverse(*n)))?;
    let sure = c as f32 / bars as f32;
    (sure >= AGREEING).then_some((n as u8, sure))
}

/// A time signature's name: beats a bar over four.
pub fn meter_name(beats: u8) -> String {
    format!("{beats}/4")
}

/// A second-order filter (RBJ's cookbook).
#[derive(Debug, Clone, Copy)]
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    x: [f32; 2],
    y: [f32; 2],
}

impl Biquad {
    fn low_pass(rate: f32, at: f32) -> Self {
        let w = 2.0 * std::f32::consts::PI * at / rate;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let a0 = 1.0 + alpha;
        let b1 = (1.0 - cos) / a0;
        Self { b: [b1 / 2.0, b1, b1 / 2.0], a: [-2.0 * cos / a0, (1.0 - alpha) / a0], x: [0.0; 2], y: [0.0; 2] }
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.b[1] * self.x[0] + self.b[2] * self.x[1] - self.a[0] * self.y[0] - self.a[1] * self.y[1];
        self.x = [x, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 22_050;

    /// `secs` of sound: a 55 Hz kick (falling fast) at each of `kicks`, a hi-hat's noise at each of `hats`.
    fn heard(secs: f64, kicks: &[f64], hats: &[f64]) -> Vec<f64> {
        let mut ear = KickEar::new(RATE);
        let mut seed = 7u64;
        for i in 0..(secs * f64::from(RATE)) as usize {
            let t = i as f64 / f64::from(RATE);
            let kick = kicks.iter().filter(|k| t >= **k && t < **k + 0.15).map(|k| ((t - k) * -30.0).exp() * (2.0 * std::f64::consts::PI * 55.0 * (t - k)).sin()).sum::<f64>();
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let noise = (seed % 2000) as f64 / 1000.0 - 1.0;
            let hat = hats.iter().filter(|h| t >= **h && t < **h + 0.03).map(|h| ((t - h) * -150.0).exp() * noise * 0.3).sum::<f64>();
            ear.hear((0.8 * kick + hat) as f32);
        }
        ear.kicks()
    }

    fn every(from: f64, step: f64, n: usize) -> Vec<f64> {
        (0..n).map(|i| from + step * i as f64).collect()
    }

    #[test]
    fn a_kick_is_found_in_the_low_band_and_a_hi_hat_is_not() {
        let kicks = every(0.5, 0.5, 20);
        let hats = every(0.75, 0.5, 20);
        let found = heard(11.0, &kicks, &hats);
        assert_eq!(found.len(), kicks.len(), "{found:?}");
        assert!(found.iter().zip(&kicks).all(|(f, k)| (f - k).abs() < 0.03), "{found:?}");
    }

    #[test]
    fn house_scores_as_four_on_the_floor_and_a_breakbeat_does_not() {
        let beats = every(0.5, 0.5, 64);
        let house = heard(33.0, &beats, &every(0.75, 0.5, 64));
        let s = four_on_the_floor(&house, &beats).unwrap();
        assert!(s >= FOUR_ON_THE_FLOOR, "house {s}");
        // A breakbeat: a kick on the first beat and the "and" of the third, each bar.
        let break_kicks: Vec<f64> = (0..16).flat_map(|bar| [0.5 + 2.0 * f64::from(bar), 1.75 + 2.0 * f64::from(bar)]).collect();
        let breakbeat = heard(33.0, &break_kicks, &[]);
        let s = four_on_the_floor(&breakbeat, &beats).unwrap();
        assert!(s < 0.35, "breakbeat {s}");
    }

    #[test]
    fn a_breakdown_does_not_count_against_four_on_the_floor_but_a_track_mostly_without_kicks_does() {
        let beats = every(0.5, 0.5, 96);
        // Kicks on every beat but a breakdown of 16 beats in the middle.
        let kicks: Vec<f64> = beats.iter().enumerate().filter(|(i, _)| !(40..56).contains(i)).map(|(_, b)| *b).collect();
        assert!(four_on_the_floor(&kicks, &beats).unwrap() > 0.9);
        // Kicks only in the last 16 beats.
        let late: Vec<f64> = beats[80..].to_vec();
        assert!(four_on_the_floor(&late, &beats).unwrap() < FOUR_ON_THE_FLOOR);
        assert_eq!(four_on_the_floor(&kicks, &beats[..10]), None, "too few beats");
        assert_eq!(four_on_the_floor(&[], &beats), Some(0.0));
    }

    #[test]
    fn the_time_signature_is_the_beats_a_bar_most_bars_have() {
        let beats = every(0.0, 0.5, 64);
        let four: Vec<f64> = beats.iter().step_by(4).copied().collect();
        assert_eq!(meter(&beats, &four), Some((4, 1.0)));
        let three: Vec<f64> = beats.iter().step_by(3).copied().collect();
        assert_eq!(meter(&beats, &three), Some((3, 1.0)));
        // Mostly four, a bar of two now and then: still 4/4, a little less sure.
        let mut mixed: Vec<f64> = Vec::new();
        let mut i = 0;
        for bar in 0..15 {
            mixed.push(beats[i]);
            i += if bar % 5 == 4 { 2 } else { 4 };
        }
        let (n, sure) = meter(&beats, &mixed).unwrap();
        assert!(n == 4 && sure < 1.0 && sure >= AGREEING, "{n} {sure}");
        assert_eq!(meter(&beats, &four[..5]), None, "too few bars");
        assert_eq!(meter_name(3), "3/4");
    }
}
