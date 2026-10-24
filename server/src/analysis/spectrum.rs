//! A track's spectrum over time, for the full-screen player's bands (the owner, 2026-10-24: "many, many meters, the way
//! Winamp drew them", and a slider from 3 to 32 bands for every look): [`BANDS`] bands spaced evenly in pitch from
//! [`LOWEST`] to [`HIGHEST`] (about a third of an octave apart), each band's level [`FPS`] times a second, as a byte in
//! decibels against the track's loudest of any band and moment (0: [`FLOOR_DB`] or more below it; 255: it). Worked out
//! the first time a track plays and kept beside its analysis (`analysis_shape.spectrum`), never by the background pass
//! (the owner: as it is played): about 38 KB a minute.
//!
//! Kept form: the format byte, [`BANDS`], [`FPS`], the frame count (u32, little-endian), then the frames one after the
//! other, each [`BANDS`] bytes, lowest band first.
//!
//! A bank of band passes, not an FFT: an FFT over a 50 ms frame cannot tell the bass bands apart. Each band is two
//! band-pass sections (`band_wave::Biquad`), run at the lowest rate it fits in: the signal is halved again and again (a
//! low pass, then every other sample), and a band runs on the most halved signal that still holds it, so the bass bands
//! cost little.
//!
//! Blocking; run on a blocking thread.

use std::path::Path;

use anyhow::{Result, bail};

use super::band_wave::{Biquad, Lr4};

pub const FORMAT: u8 = 1;
pub const BANDS: usize = 32;
/// Frames a second: a meter's snap, about Winamp's.
pub const FPS: u32 = 20;
/// The lowest and highest bands' centres (Hz).
pub const LOWEST: f32 = 25.0;
pub const HIGHEST: f32 = 16_000.0;
/// How far below the loudest a byte of 0 is (dB).
pub const FLOOR_DB: f32 = 60.0;
/// Kept in its place for a file it could not be worked out of: not tried again until the file changes.
pub const FAILED: u8 = 0x80 | FORMAT;
const HEAD: usize = 7;
/// Each band pass section's sharpness: two in a row give about the bands' spacing, so neighbours meet without a gap.
const Q: f32 = 3.0;
/// How often the signal may be halved: the lowest band is under a sixth of the rate after this many.
const HALVINGS: usize = 7;

/// A band's centre (Hz).
pub fn centre(band: usize) -> f32 {
    LOWEST * (HIGHEST / LOWEST).powf(band as f32 / (BANDS - 1) as f32)
}

/// A track's spectrum, as kept: see the module.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Spectrum(pub Vec<u8>);

impl Spectrum {
    /// Whether it is of the form this Cratarr makes, whole.
    pub fn current(&self) -> bool {
        self.0.len() >= HEAD && self.0[0] == FORMAT && usize::from(self.0[1]) == BANDS && u32::from(self.0[2]) == FPS && self.0.len() == HEAD + self.frames() * BANDS
    }

    /// Whether this is the mark of a file it could not be worked out of ([`FAILED`]).
    pub fn failed(&self) -> bool {
        self.0 == [FAILED]
    }

    pub fn frames(&self) -> usize {
        self.0.get(3..HEAD).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    }

    /// A frame's bands, lowest first.
    #[cfg(test)]
    fn frame(&self, i: usize) -> &[u8] {
        &self.0[HEAD + i * BANDS..HEAD + (i + 1) * BANDS]
    }
}

struct Band {
    filter: [Biquad; 2],
    sum: f64,
    count: u32,
}

/// The bank: the signal halved again and again, each band on its own rate, its energy summed a frame at a time.
struct Bank {
    /// Per halving: its low pass before every other sample goes on, and whether the next one does.
    halve: Vec<(Lr4, bool)>,
    /// The bands each rate runs (index: how often halved).
    at: Vec<Vec<usize>>,
    bands: Vec<Band>,
    frame: usize,
    filled: usize,
    /// Each frame's mean squares, [`BANDS`] a frame.
    frames: Vec<f32>,
}

impl Bank {
    fn new(rate: u32) -> Self {
        let r = rate as f32;
        let mut at = vec![Vec::new(); HALVINGS + 1];
        let mut bands = Vec::with_capacity(BANDS);
        for b in 0..BANDS {
            let hz = centre(b);
            // The most halved rate still six times the band's centre (well below where the halving's low pass cuts).
            let k = (0..=HALVINGS).rev().find(|&k| hz * 6.0 < r / (1u32 << k) as f32).unwrap_or(0);
            let rk = r / (1u32 << k) as f32;
            let section = Biquad::band_pass(rk, hz, Q);
            at[k].push(b);
            bands.push(Band { filter: [section, section], sum: 0.0, count: 0 });
        }
        // Each halving's low pass at an eighth of its rate: what would fold onto the bands below is 40 dB down or more.
        let halve = (0..HALVINGS).map(|k| (Lr4::new(r / (1u32 << k) as f32, r / (1u32 << k) as f32 / 8.0, false), false)).collect();
        let deepest = at.iter().rposition(|v| !v.is_empty()).unwrap_or(0);
        at.truncate(deepest + 1);
        Bank { halve, at, bands, frame: (rate / FPS).max(1) as usize, filled: 0, frames: Vec::new() }
    }

    fn hear(&mut self, x: f32) {
        let mut v = x;
        for k in 0..self.at.len() {
            for &b in &self.at[k] {
                let band = &mut self.bands[b];
                let y = band.filter[0].run(v);
                let y = band.filter[1].run(y);
                band.sum += f64::from(y) * f64::from(y);
                band.count += 1;
            }
            if k + 1 == self.at.len() {
                break;
            }
            let (low, on) = &mut self.halve[k];
            v = low.run(v);
            *on = !*on;
            if !*on {
                break;
            }
        }
        self.filled += 1;
        if self.filled == self.frame {
            self.close();
        }
    }

    fn close(&mut self) {
        for band in &mut self.bands {
            let ms = if band.count > 0 { (band.sum / f64::from(band.count)) as f32 } else { 0.0 };
            self.frames.push(ms);
            band.sum = 0.0;
            band.count = 0;
        }
        self.filled = 0;
    }

    fn finish(mut self) -> Spectrum {
        // A last part frame counts when it is at least half one.
        if self.filled * 2 >= self.frame {
            self.close();
        }
        let n = self.frames.len() / BANDS;
        let loudest = self.frames.iter().copied().fold(0.0f32, f32::max);
        let mut out = Vec::with_capacity(HEAD + n * BANDS);
        out.extend([FORMAT, BANDS as u8, FPS as u8]);
        out.extend((n as u32).to_le_bytes());
        out.extend(self.frames.iter().map(|&ms| {
            if loudest <= 0.0 || ms <= 0.0 {
                return 0;
            }
            let db = 10.0 * (ms / loudest).log10();
            ((db + FLOOR_DB) / FLOOR_DB * 255.0).round().clamp(0.0, 255.0) as u8
        }));
        Spectrum(out)
    }
}

/// A file's spectrum, from one decode of it (the analysis's own reader, every format symphonia reads).
pub fn work_out(path: &Path) -> Result<Spectrum> {
    let mut mono = super::Mono::open(path)?;
    if mono.rate() < 8_000 {
        bail!("{} Hz is too low a rate", mono.rate());
    }
    let mut bank = Bank::new(mono.rate());
    let mut samples = Vec::new();
    while mono.read(&mut samples)? {
        samples.iter().for_each(|&x| bank.hear(x));
        samples.clear();
    }
    Ok(bank.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(path: &Path, rate: u32, secs: f64, tones: &[(f64, f64)]) {
        let n = (f64::from(rate) * secs) as u32;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + n * 2).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * 2).to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(n * 2).to_le_bytes());
        for i in 0..n {
            let t = f64::from(i) / f64::from(rate);
            let v: f64 = tones.iter().map(|(hz, a)| a * (2.0 * std::f64::consts::PI * hz * t).sin()).sum();
            b.extend_from_slice(&((v * 32767.0) as i16).to_le_bytes());
        }
        std::fs::write(path, b).unwrap();
    }

    /// The band whose centre is nearest `hz`.
    fn nearest(hz: f32) -> usize {
        (0..BANDS).min_by(|&a, &b| (centre(a) / hz).ln().abs().total_cmp(&(centre(b) / hz).ln().abs())).unwrap()
    }

    #[test]
    fn each_tone_peaks_in_its_own_band_twenty_frames_a_second() {
        let dir = std::env::temp_dir().join(format!("cratarr-spectrum-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for hz in [40.0, 250.0, 1000.0, 6000.0, 14000.0] {
            let path = dir.join(format!("{hz}.wav"));
            wav(&path, 44_100, 2.0, &[(f64::from(hz), 0.5)]);
            let s = work_out(&path).unwrap();
            assert!(s.current(), "{:?}", &s.0[..HEAD]);
            assert_eq!(s.frames(), 40, "2 s at {FPS} a second");
            // Past the filters' settling: the loudest band is the tone's, and bands far from it are far below.
            let f = s.frame(30);
            let top = (0..BANDS).max_by_key(|&b| f[b]).unwrap();
            assert!(top.abs_diff(nearest(hz)) <= 1, "{hz} Hz peaked in band {top} ({} Hz)", centre(top));
            let far = if top > BANDS / 2 { 0 } else { BANDS - 1 };
            assert!(f[far] < 120, "{hz} Hz: band {far} at {}", f[far]);
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn levels_are_decibels_against_the_loudest_and_follow_the_track() {
        let dir = std::env::temp_dir().join(format!("cratarr-spectrum-db-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // The same tone at full and at a tenth (20 dB down), a second each.
        let (loud, quiet) = (dir.join("loud.wav"), dir.join("quiet.wav"));
        wav(&loud, 44_100, 1.0, &[(1000.0, 0.5)]);
        wav(&quiet, 44_100, 1.0, &[(1000.0, 0.05)]);
        let mut joined = std::fs::read(&loud).unwrap();
        let tail = std::fs::read(&quiet).unwrap()[44..].to_vec();
        let n = (joined.len() - 44 + tail.len()) as u32;
        joined.extend(tail);
        joined[4..8].copy_from_slice(&(36 + n).to_le_bytes());
        joined[40..44].copy_from_slice(&n.to_le_bytes());
        let path = dir.join("both.wav");
        std::fs::write(&path, joined).unwrap();
        let s = work_out(&path).unwrap();
        let b = nearest(1000.0);
        let (first, second) = (s.frame(15)[b], s.frame(35)[b]);
        assert!(first >= 250, "the loudest is near 255: {first}");
        // 20 dB of a 60 dB floor is a third of the bytes down.
        let drop = f32::from(first) - f32::from(second);
        assert!((drop - 255.0 / 3.0).abs() < 12.0, "{first} then {second}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Real files (comma-separated in CRATARR_SPECTRUM): each one's spectrum, its time and its bands' means printed.
    /// Run with `--ignored`.
    #[test]
    #[ignore]
    fn real_files_for_the_lab() {
        for input in std::env::var("CRATARR_SPECTRUM").unwrap_or_default().split(',').filter(|p| !p.is_empty()) {
            let started = std::time::Instant::now();
            let s = work_out(Path::new(input)).unwrap();
            let n = s.frames().max(1);
            let means: Vec<u32> = (0..BANDS).map(|b| (0..n).map(|f| u32::from(s.0[HEAD + f * BANDS + b])).sum::<u32>() / n as u32).collect();
            println!("{input}: {} frames in {:?}, {} bytes; band means {means:?}", s.frames(), started.elapsed(), s.0.len());
            assert!(s.current());
        }
    }
}
