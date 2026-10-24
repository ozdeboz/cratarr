//! The waveform in three bands (the owner, 2026-10-24: the full-screen player's three-band display): a track's sound
//! split by crossovers at 150 Hz and 2.5 kHz (the DJ programs' split: kick and bass, the voice and most instruments,
//! cymbals and air), adjacent so nothing is left out, and over the whole track in 512 bins, as the waveform
//! (`Analysis::waveform`) has them.
//!
//! Each band is the mix's middle (left and right averaged) through a Linkwitz-Riley crossover of the fourth order (two
//! Butterworth biquads at each edge, 24 dB an octave): low below 150 Hz, mid between, high above 2.5 kHz. A bin holds
//! each band's RMS over its share of the track, as a byte against the loudest bin of any band, so the bands stay in
//! proportion to each other (a quiet treble looks quiet); the display chooses how to draw them.
//!
//! Kept beside the analysis (`analysis_shape.band_wave`): a format byte, then the 512 low bins, the 512 mid and the
//! 512 high. Worked out the first time a track plays, and for the rest of the library by the analysis pass when it has
//! nothing else to do (`app::analysis_pass`).
//!
//! Blocking; run on a blocking thread.

use std::path::Path;

use anyhow::Result;

/// The kept form's first byte.
pub const FORMAT: u8 = 1;
/// Bins over the whole track, as the waveform.
pub const BINS: usize = 512;
/// The crossovers (Hz), low to high.
pub const EDGES: [f32; 2] = [150.0, 2500.0];
/// The kept form's length: the format byte and three bands of bins.
pub const LEN: usize = 1 + 3 * BINS;
/// Kept in its place for a file this Cratarr could not work them out of: not tried again, by the pass or a play, until
/// the file changes (its analysis then goes, and this with it), so a file that fails costs one decode, not one a start.
pub const FAILED: u8 = 0x80 | FORMAT;

/// A track's waveform in three bands, as kept: see the module.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BandWave(pub Vec<u8>);

impl BandWave {
    /// Whether these are of the form this Cratarr makes (an older one counts as not worked out).
    pub fn current(&self) -> bool {
        self.0.len() == LEN && self.0[0] == FORMAT
    }

    /// Whether this is the mark of a file they could not be worked out of ([`FAILED`]).
    pub fn failed(&self) -> bool {
        self.0 == [FAILED]
    }
}

/// One second-order section (RBJ's cookbook), direct form I; the spectrum's filters too (`analysis::spectrum`).
#[derive(Clone, Copy)]
pub(super) struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    x: [f32; 2],
    y: [f32; 2],
}

impl Biquad {
    /// A Butterworth low or high pass at `hz` (Q = 1/√2).
    fn new(rate: f32, hz: f32, high: bool) -> Self {
        let w = 2.0 * std::f32::consts::PI * (hz / rate).min(0.49);
        let (sin, cos) = w.sin_cos();
        let alpha = sin / std::f32::consts::SQRT_2;
        let a0 = 1.0 + alpha;
        let (b0, b1) = if high { ((1.0 + cos) / 2.0, -(1.0 + cos)) } else { ((1.0 - cos) / 2.0, 1.0 - cos) };
        Biquad { b: [b0 / a0, b1 / a0, b0 / a0], a: [-2.0 * cos / a0, (1.0 - alpha) / a0], x: [0.0; 2], y: [0.0; 2] }
    }

    /// A band pass around `hz`, its peak at 0 dB, `q` sharp.
    pub(super) fn band_pass(rate: f32, hz: f32, q: f32) -> Self {
        let w = 2.0 * std::f32::consts::PI * (hz / rate).min(0.49);
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        Biquad { b: [alpha / a0, 0.0, -alpha / a0], a: [-2.0 * cos / a0, (1.0 - alpha) / a0], x: [0.0; 2], y: [0.0; 2] }
    }

    pub(super) fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.b[1] * self.x[0] + self.b[2] * self.x[1] - self.a[0] * self.y[0] - self.a[1] * self.y[1];
        self.x = [x, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }
}

/// A fourth-order Linkwitz-Riley pass: the same Butterworth section twice.
#[derive(Clone, Copy)]
pub(super) struct Lr4([Biquad; 2]);

impl Lr4 {
    pub(super) fn new(rate: f32, hz: f32, high: bool) -> Self {
        let s = Biquad::new(rate, hz, high);
        Lr4([s, s])
    }

    pub(super) fn run(&mut self, x: f32) -> f32 {
        let y = self.0[0].run(x);
        self.0[1].run(y)
    }
}

/// The three bands of a stream of samples, their energy summed a short block at a time.
struct Splitter {
    low: Lr4,
    mid_high: Lr4,
    mid_low: Lr4,
    high: Lr4,
    /// Samples a block, and the samples in the block now.
    block: usize,
    filled: usize,
    sums: [f64; 3],
    /// Each band's energy, a block at a time, and how many samples each block holds.
    blocks: [Vec<f32>; 3],
    counts: Vec<u32>,
}

impl Splitter {
    fn new(rate: u32) -> Self {
        let r = rate as f32;
        Splitter {
            low: Lr4::new(r, EDGES[0], false),
            mid_high: Lr4::new(r, EDGES[0], true),
            mid_low: Lr4::new(r, EDGES[1], false),
            high: Lr4::new(r, EDGES[1], true),
            // 10 ms: a 40-minute track keeps under 3 MB of blocks before they are binned.
            block: (rate as usize / 100).max(1),
            filled: 0,
            sums: [0.0; 3],
            blocks: [Vec::new(), Vec::new(), Vec::new()],
            counts: Vec::new(),
        }
    }

    fn hear(&mut self, x: f32) {
        let low = self.low.run(x);
        let mid = self.mid_low.run(self.mid_high.run(x));
        let high = self.high.run(x);
        for (s, v) in self.sums.iter_mut().zip([low, mid, high]) {
            *s += f64::from(v) * f64::from(v);
        }
        self.filled += 1;
        if self.filled == self.block {
            self.close();
        }
    }

    fn close(&mut self) {
        if self.filled == 0 {
            return;
        }
        for (b, s) in self.blocks.iter_mut().zip(self.sums) {
            b.push(s as f32);
        }
        self.counts.push(self.filled as u32);
        self.sums = [0.0; 3];
        self.filled = 0;
    }

    /// The blocks binned: each bin's RMS per band, as bytes against the loudest bin of any band.
    fn finish(mut self) -> BandWave {
        self.close();
        let n = self.counts.len();
        let mut rms = [[0f32; BINS]; 3];
        if n > 0 {
            for i in 0..BINS {
                // A bin's share of the blocks; a track shorter than 512 blocks repeats its blocks over the bins.
                let from = i * n / BINS;
                let to = ((i + 1) * n / BINS).max(from + 1).min(n);
                let samples: u64 = self.counts[from..to].iter().map(|&c| u64::from(c)).sum();
                for band in 0..3 {
                    let energy: f64 = self.blocks[band][from..to].iter().map(|&e| f64::from(e)).sum();
                    rms[band][i] = (energy / samples.max(1) as f64).sqrt() as f32;
                }
            }
        }
        let loudest = rms.iter().flatten().fold(0f32, |m, &v| m.max(v));
        let mut out = Vec::with_capacity(LEN);
        out.push(FORMAT);
        for band in &rms {
            out.extend(band.iter().map(|&v| if loudest > 0.0 { (v / loudest * 255.0).round().clamp(0.0, 255.0) as u8 } else { 0 }));
        }
        BandWave(out)
    }
}

/// A file's waveform in three bands, from one decode of it: the analysis's own mono reader (`super::Mono`), which
/// reads every format symphonia does, MP3s behind a large ID3 tag and damaged frames included. Not the converter's
/// `Source`: that one is for lossless copies and Opus streams, and took no MP3 (0.8.3: no bands for 25k of 31k).
pub fn work_out(path: &Path) -> Result<BandWave> {
    let mut mono = super::Mono::open(path)?;
    let mut split = Splitter::new(mono.rate());
    let mut frames = Vec::new();
    while mono.read(&mut frames)? {
        frames.iter().for_each(|&x| split.hear(x));
        frames.clear();
    }
    Ok(split.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 16-bit stereo WAV of `tones` (Hz, amplitude) summed, `secs` long.
    fn wav(path: &Path, rate: u32, secs: f64, tones: &[(f64, f64)]) {
        let n = (f64::from(rate) * secs) as u32;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + n * 4).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * 4).to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(n * 4).to_le_bytes());
        for i in 0..n {
            let t = f64::from(i) / f64::from(rate);
            let v: f64 = tones.iter().map(|(hz, a)| a * (2.0 * std::f64::consts::PI * hz * t).sin()).sum();
            let s = (v * 32767.0) as i16;
            b.extend_from_slice(&s.to_le_bytes());
            b.extend_from_slice(&s.to_le_bytes());
        }
        std::fs::write(path, b).unwrap();
    }

    fn mean(v: &[u8]) -> f64 {
        v.iter().map(|&x| f64::from(x)).sum::<f64>() / v.len() as f64
    }

    #[test]
    fn each_tone_lands_in_its_own_band() {
        let dir = std::env::temp_dir().join(format!("cratarr-bandwave-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (hz, band) in [(60.0, 0), (700.0, 1), (8000.0, 2)] {
            let path = dir.join(format!("{hz}.wav"));
            wav(&path, 44_100, 2.0, &[(hz, 0.5)]);
            let w = work_out(&path).unwrap();
            assert!(w.current() && w.0.len() == LEN);
            let bands: Vec<f64> = (0..3).map(|b| mean(&w.0[1 + b * BINS..1 + (b + 1) * BINS])).collect();
            assert!(bands[band] > 200.0, "{hz} Hz: its band near the loudest, {bands:?}");
            for (other, v) in bands.iter().enumerate().filter(|(o, _)| *o != band) {
                assert!(*v < bands[band] * 0.2, "{hz} Hz: band {other} well below, {bands:?}");
            }
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_bands_keep_their_proportions_and_follow_the_track_in_time() {
        let dir = std::env::temp_dir().join(format!("cratarr-bandwave-mix-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // A loud bass and a quiet treble: the treble stays small against the bass, not stretched to its own loudest.
        let path = dir.join("mix.wav");
        wav(&path, 44_100, 2.0, &[(60.0, 0.6), (8000.0, 0.06)]);
        let w = work_out(&path).unwrap();
        let (low, high) = (mean(&w.0[1..1 + BINS]), mean(&w.0[1 + 2 * BINS..]));
        assert!(high < low * 0.2, "low {low}, high {high}");
        // Silence then a tone: the first half of the bins near nothing, the second half not.
        let half = dir.join("half.wav");
        let rate = 44_100u32;
        wav(&half, rate, 1.0, &[]);
        let mut silent = std::fs::read(&half).unwrap();
        wav(&half, rate, 1.0, &[(700.0, 0.5)]);
        let tone = std::fs::read(&half).unwrap();
        let data = |b: &[u8]| b[44..].to_vec();
        let mut joined = data(&silent);
        joined.extend(data(&tone));
        let n = (joined.len() as u32).to_le_bytes();
        silent.truncate(44);
        silent[40..44].copy_from_slice(&n);
        silent[4..8].copy_from_slice(&(36 + joined.len() as u32).to_le_bytes());
        silent.extend(joined);
        std::fs::write(&half, silent).unwrap();
        let w = work_out(&half).unwrap();
        let mid = &w.0[1 + BINS..1 + 2 * BINS];
        assert!(mean(&mid[..240]) < 5.0 && mean(&mid[272..]) > 200.0, "{} then {}", mean(&mid[..240]), mean(&mid[272..]));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Real files (comma-separated in CRATARR_BANDS: MP3s behind a large cover, AAC, Vorbis): each one's bands worked out,
    /// with each band's mean printed. Run with `--ignored`.
    #[test]
    #[ignore]
    fn real_files_for_the_lab() {
        for input in std::env::var("CRATARR_BANDS").unwrap_or_default().split(',').filter(|p| !p.is_empty()) {
            let w = work_out(Path::new(input)).unwrap();
            let m = |i: usize| mean(&w.0[1 + i * BINS..1 + (i + 1) * BINS]);
            println!("{input}: low {:.0}, mid {:.0}, high {:.0}", m(0), m(1), m(2));
            assert!(m(0) + m(1) + m(2) > 10.0, "{input} came out silent");
        }
    }
}
