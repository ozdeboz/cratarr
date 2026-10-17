//! A track's sound in three bands, for the visuals (decision 0046, amended): low (kick drum and bass, below 150 Hz),
//! middle (snare, voices, most instruments: 300 Hz to 2.5 kHz) and high (hats and cymbals, above 5 kHz). For each, its
//! loudness every 50 ms and its transients: each sudden start, with its strength. And the whole sound's loudness as
//! heard, every 100 ms: EBU R128's momentary loudness (LUFS), which the show's intensity follows. Worked out from the same decode as the
//! rest of the analysis, only when the visuals ask (`analyse_with_bands`): some 20 KB a track, not for the whole
//! library.

/// The bands, low to high.
pub const BANDS: usize = 3;
/// Loudness values a second.
pub const LOUDNESS_RATE: usize = 20;
/// The kept form's first byte. 1: before the loudness as heard was kept (read as not worked out).
pub const FORMAT: u8 = 2;
/// The loudness as heard: values a second, and its scale (a byte is a third of a dB from -70 LUFS: 255 is +15).
pub const MOMENTS_RATE: usize = 10;
const MOMENT_FLOOR: f32 = -70.0;
const MOMENT_STEPS: f32 = 3.0;
/// The filters' corners (Hz): low below the first, middle between the second and third, high above the fourth.
const LOW_BELOW: f64 = 150.0;
const MIDDLE_FROM: f64 = 300.0;
const MIDDLE_BELOW: f64 = 2500.0;
const HIGH_FROM: f64 = 5000.0;
/// Measured every 10 ms.
const FRAMES_A_SECOND: usize = 100;
/// A band's loudness is kept over this many dB under its loudest (its 98th percentile): 0 at the bottom, 255 at the top.
const RANGE_DB: f32 = 30.0;
/// A transient: the band's level rises at least this much within 20 ms (dB), stands out of the rises around it, and is
/// no quieter than this far under the band's loudest.
const RISE_DB: f32 = 6.0;
const QUIET_DB: f32 = 35.0;
/// Starts closer than this to the last one in their band are one (ms), by band: a kick rings longer than a hat.
const GAP_MS: [usize; BANDS] = [120, 90, 60];

/// A second-order filter (RBJ's cookbook, Butterworth Q).
#[derive(Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}

impl Biquad {
    fn new(rate: u32, corner: f64, high: bool) -> Self {
        let rate = f64::from(rate);
        let w = 2.0 * std::f64::consts::PI * corner.min(0.45 * rate) / rate;
        let (sin, cos) = w.sin_cos();
        let alpha = sin * std::f64::consts::FRAC_1_SQRT_2;
        let a0 = 1.0 + alpha;
        let b = if high { [(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0] } else { [(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0] };
        Biquad { b: b.map(|x| x / a0), a: [-2.0 * cos / a0, (1.0 - alpha) / a0], z: [0.0; 2] }
    }

    /// Passes everything as it is.
    fn through() -> Self {
        Biquad { b: [1.0, 0.0, 0.0], a: [0.0; 2], z: [0.0; 2] }
    }

    /// Transposed direct form II.
    fn run(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// Listens in bands: sound in, sample by sample (mono), each band's energy every 10 ms. Each band's edges fall at
/// 24 dB an octave (two second-order filters at each edge, worked out for the file's own rate).
pub struct BandEar {
    /// Four filters a band, in a row.
    filters: [[Biquad; 4]; BANDS],
    hop: usize,
    at: usize,
    sum: [f64; BANDS],
    energy: Vec<[f32; BANDS]>,
}

impl BandEar {
    pub fn new(rate: u32) -> Self {
        let f = |corner, high| Biquad::new(rate, corner, high);
        let all = Biquad::through();
        BandEar {
            filters: [
                [f(LOW_BELOW, false), f(LOW_BELOW, false), all, all],
                [f(MIDDLE_FROM, true), f(MIDDLE_FROM, true), f(MIDDLE_BELOW, false), f(MIDDLE_BELOW, false)],
                [f(HIGH_FROM, true), f(HIGH_FROM, true), all, all],
            ],
            hop: (rate as usize / FRAMES_A_SECOND).max(1),
            at: 0,
            sum: [0.0; BANDS],
            energy: Vec::new(),
        }
    }

    pub fn hear(&mut self, sample: f32) {
        let x = f64::from(sample);
        for (sum, chain) in self.sum.iter_mut().zip(self.filters.iter_mut()) {
            let y = chain.iter_mut().fold(x, |v, f| f.run(v));
            *sum += y * y;
        }
        self.at += 1;
        if self.at == self.hop {
            let hop = self.hop as f64;
            self.energy.push(self.sum.map(|e| (e / hop) as f32));
            (self.sum, self.at) = ([0.0; BANDS], 0);
        }
    }

    /// The bands, with `moments`: the momentary loudness (LUFS) of each 400 ms window ending every 100 ms from the
    /// fourth on.
    pub fn finish(self, moments: &[f32]) -> Bands {
        Bands::of(&self.energy, moments)
    }
}

/// A track's bands as kept and served: a format byte (2), the number of bands (3), loudness values a second (20), the
/// loudness as heard's values a second (10); the loudness frames (u32, little-endian), then that many frames of one
/// byte a band (low, middle, high: 0 for 30 dB or more under the band's loudest, 255 for its loudest); then for each band
/// its transients: their number (u32), then each one's time in milliseconds (u32) and strength (0..=255, 255 for the
/// band's strong ones); then the loudness as heard: its number of values (u32), then one byte each, the momentary
/// loudness of the 400 ms around that tenth of a second in thirds of a dB from -70 LUFS (0: -70 or quieter).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bands(pub Vec<u8>);

/// What `Bands` holds, read back (tests, and anything that looks inside).
#[derive(Debug, Default, PartialEq)]
pub struct Read {
    /// Each band's loudness, `LOUDNESS_RATE` a second, 0..=255.
    pub loudness: Vec<[u8; BANDS]>,
    /// Each band's transients: (milliseconds, strength).
    pub transients: [Vec<(u32, u8)>; BANDS],
    /// The loudness as heard (LUFS), `MOMENTS_RATE` a second.
    pub moments: Vec<f32>,
}

impl Bands {
    fn of(energy: &[[f32; BANDS]], moments: &[f32]) -> Self {
        let levels: Vec<Vec<f32>> = (0..BANDS).map(|b| energy.iter().map(|e| db(e[b])).collect()).collect();
        // Each band's loudest: its 98th percentile, among the frames with any sound in that band.
        let tops: Vec<f32> = levels.iter().map(|l| percentile(l.iter().copied().filter(|v| *v > -100.0).collect(), 0.98).unwrap_or(-100.0)).collect();
        let per = FRAMES_A_SECOND / LOUDNESS_RATE;
        let frames = energy.len().div_ceil(per);
        let mut out = vec![FORMAT, BANDS as u8, LOUDNESS_RATE as u8, MOMENTS_RATE as u8];
        out.extend_from_slice(&(frames as u32).to_le_bytes());
        for f in 0..frames {
            let span = &energy[f * per..((f + 1) * per).min(energy.len())];
            for (b, top) in tops.iter().enumerate() {
                let level = db(span.iter().map(|e| e[b]).sum::<f32>() / span.len() as f32);
                // Nothing in the band (digital silence) is its bottom, however quiet the band is all through.
                let v = if level > -100.0 { ((level - (top - RANGE_DB)) / RANGE_DB).clamp(0.0, 1.0) } else { 0.0 };
                out.push((v * 255.0).round() as u8);
            }
        }
        for (b, level) in levels.iter().enumerate() {
            let hits = transients(level, tops[b], GAP_MS[b]);
            out.extend_from_slice(&(hits.len() as u32).to_le_bytes());
            for (ms, strength) in hits {
                out.extend_from_slice(&ms.to_le_bytes());
                out.push(strength);
            }
        }
        // Each window by its middle: the first ends 400 ms in, around 200 ms; the two before it are its value too.
        let centred: Vec<f32> = moments.first().map(|m| [*m, *m]).into_iter().flatten().chain(moments.iter().copied()).collect();
        out.extend_from_slice(&(centred.len() as u32).to_le_bytes());
        out.extend(centred.iter().map(|m| if m.is_finite() { ((m - MOMENT_FLOOR) * MOMENT_STEPS).clamp(0.0, 255.0).round() as u8 } else { 0 }));
        Bands(out)
    }

    /// What it holds; None for bytes not in this form.
    pub fn read(&self) -> Option<Read> {
        let b = &self.0;
        if b.len() < 8 || b[0] != FORMAT || usize::from(b[1]) != BANDS {
            return None;
        }
        let u32_at = |i: usize| b.get(i..i + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]));
        let frames = u32_at(4)? as usize;
        let mut at = 8;
        let loudness = (0..frames)
            .map(|f| {
                let s = b.get(8 + f * BANDS..8 + (f + 1) * BANDS)?;
                Some([s[0], s[1], s[2]])
            })
            .collect::<Option<Vec<_>>>()?;
        at += frames * BANDS;
        let mut transients: [Vec<(u32, u8)>; BANDS] = Default::default();
        for t in transients.iter_mut() {
            let n = u32_at(at)? as usize;
            at += 4;
            for _ in 0..n {
                t.push((u32_at(at)?, *b.get(at + 4)?));
                at += 5;
            }
        }
        let n = u32_at(at)? as usize;
        at += 4;
        let moments = b.get(at..at + n)?.iter().map(|v| MOMENT_FLOOR + f32::from(*v) / MOMENT_STEPS).collect();
        at += n;
        (at == b.len()).then_some(Read { loudness, transients, moments })
    }
}

fn db(energy: f32) -> f32 {
    10.0 * (energy + 1e-12).log10()
}

fn percentile(mut v: Vec<f32>, at: f64) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f32::total_cmp);
    Some(v[((v.len() - 1) as f64 * at).round() as usize])
}

/// A band's transients from its level every 10 ms (dB): where the level rises fastest within 20 ms, by at least
/// `RISE_DB`, more than the rises around it (the second on either side) usually do, while the band is not quiet; one per
/// `gap_ms`, the strongest. Each at the middle of the 10 ms its rise ends in, less one: a sound shows fully in the frame
/// after the one it starts in. Strength against the band's strong transients (the tenth strongest and up are 255).
fn transients(level: &[f32], top: f32, gap_ms: usize) -> Vec<(u32, u8)> {
    let n = level.len();
    let rise: Vec<f32> = (0..n).map(|i| if i >= 2 { (level[i] - level[i - 2]).max(0.0) } else { 0.0 }).collect();
    let mut sums = vec![0f64; n + 1];
    for i in 0..n {
        sums[i + 1] = sums[i] + f64::from(rise[i]);
    }
    let gap = gap_ms * FRAMES_A_SECOND / 1000;
    let mut picked: Vec<(usize, f32)> = Vec::new();
    for i in 2..n {
        let r = rise[i];
        if r < RISE_DB || level[i] < top - QUIET_DB {
            continue;
        }
        // The top of its rise: none larger within 30 ms, and the first of equals.
        let (lo, hi) = (i.saturating_sub(3), (i + 4).min(n));
        if rise[lo..i].iter().any(|&x| x >= r) || rise[i + 1..hi].iter().any(|&x| x > r) {
            continue;
        }
        let (from, to) = (i.saturating_sub(FRAMES_A_SECOND), (i + FRAMES_A_SECOND + 1).min(n));
        let around = ((sums[to] - sums[from]) / (to - from) as f64) as f32;
        if r < 1.5 * around + 3.0 {
            continue;
        }
        match picked.last_mut() {
            Some(last) if i - last.0 < gap => {
                if r > last.1 {
                    *last = (i, r);
                }
            }
            _ => picked.push((i, r)),
        }
    }
    let strong = percentile(picked.iter().map(|p| p.1).collect(), 0.9).unwrap_or(1.0).max(1e-3);
    picked.into_iter().map(|(i, r)| ((i * 10).saturating_sub(5) as u32, (r / strong * 255.0).clamp(0.0, 255.0).round() as u8)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    /// `secs` of sound at `rate`, each sample from `f(t)`, through a band ear.
    fn heard(rate: u32, secs: f64, f: impl Fn(f64) -> f64) -> Read {
        let mut ear = BandEar::new(rate);
        for i in 0..(secs * f64::from(rate)) as usize {
            ear.hear(f(i as f64 / f64::from(rate)) as f32);
        }
        ear.finish(&[]).read().expect("reads back")
    }

    #[test]
    fn a_tone_lands_in_its_own_band_at_any_rate() {
        for rate in [44_100, 96_000] {
            // Two seconds each: 60 Hz (low), 1 kHz (middle), 9 kHz (high).
            let r = heard(rate, 6.0, |t| 0.3 * (2.0 * PI * if t < 2.0 { 60.0 } else if t < 4.0 { 1000.0 } else { 9000.0 } * t).sin());
            assert_eq!(r.loudness.len(), 120, "20 a second");
            for (section, band) in [(0, 0), (1, 1), (2, 2)] {
                // The middle second of each section, away from the switches.
                for frame in &r.loudness[section * 40 + 10..section * 40 + 30] {
                    for (b, &v) in frame.iter().enumerate() {
                        if b == band {
                            assert!(v >= 250, "{rate} Hz, section {section}: its own band {b} at {v}");
                        } else {
                            assert!(v <= 40, "{rate} Hz, section {section}: band {b} at {v} (leaks)");
                        }
                    }
                }
            }
        }
    }

    /// A burst at `start`: a tone (or a few) under a rise of `attack` seconds and a decay of `ring`.
    fn burst(t: f64, start: f64, attack: f64, ring: f64, tones: &[f64]) -> f64 {
        let into = t - start;
        if !(0.0..ring * 6.0).contains(&into) {
            return 0.0;
        }
        let rise = (into / attack).min(1.0);
        let shape = 0.5 - 0.5 * (PI * rise).cos();
        shape * (-into / ring).exp() * tones.iter().map(|f| (2.0 * PI * f * t).sin()).sum::<f64>() / tones.len() as f64
    }

    #[test]
    fn a_kick_a_snare_and_hats_are_each_found_in_their_own_band() {
        // 120 BPM over 12 s: a kick (55 Hz, rising over 8 ms as a kick drum does) on every beat, a snare (900 and
        // 1300 Hz) on two and four, hats (7.5, 9 and 11 kHz) on the eighths between.
        let kicks: Vec<f64> = (0..24).map(|k| 0.5 + 0.5 * f64::from(k)).collect();
        let snares: Vec<f64> = (0..12).map(|k| 1.0 + 1.0 * f64::from(k)).collect();
        let hats: Vec<f64> = (0..24).map(|k| 0.75 + 0.5 * f64::from(k)).collect();
        for rate in [44_100, 96_000] {
            let r = heard(rate, 12.5, |t| {
                let near = |at: &[f64]| at.iter().copied().filter(|s| *s <= t).next_back();
                0.5 * near(&kicks).map_or(0.0, |s| burst(t, s, 0.008, 0.08, &[55.0]))
                    + 0.3 * near(&snares).map_or(0.0, |s| burst(t, s, 0.002, 0.05, &[900.0, 1300.0]))
                    + 0.2 * near(&hats).map_or(0.0, |s| burst(t, s, 0.001, 0.01, &[7500.0, 9000.0, 11_000.0]))
            });
            for (band, want) in [(0, &kicks), (1, &snares), (2, &hats)] {
                let got: Vec<f64> = r.transients[band].iter().map(|(ms, _)| f64::from(*ms) / 1000.0).collect();
                let off: Vec<f64> = got.iter().map(|g| want.iter().map(|w| (g - w).abs()).fold(f64::MAX, f64::min)).collect();
                assert!(off.iter().all(|o| *o <= 0.015), "{rate} Hz, band {band}: every transient within 15 ms of its own hits: {off:?}");
                let caught = want.iter().filter(|w| got.iter().any(|g| (g - *w).abs() <= 0.015)).count();
                assert!(caught * 10 >= want.len() * 9, "{rate} Hz, band {band}: {caught} of {}", want.len());
            }
        }
    }

    #[test]
    fn silence_has_quiet_bands_and_no_transients() {
        let r = heard(44_100, 2.0, |_| 0.0);
        assert_eq!(r.loudness.len(), 40);
        assert!(r.loudness.iter().all(|f| f.iter().all(|v| *v == 0)), "{:?}", &r.loudness[..2]);
        assert!(r.transients.iter().all(|t| t.is_empty()));
        assert_eq!(Bands::default().read(), None, "nothing (a partial decode) reads as nothing");
    }
}
