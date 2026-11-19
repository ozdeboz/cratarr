//! A track's sound as the visuals use it (decision 0046, amended: every signal on every aspect of the show), worked out
//! from the same decode as the rest of the analysis, only when the visuals ask (`analyse_with_signals`), and kept as
//! format 3: a list of self-describing sections (`Signals`), so a later signal is another section, not another format.
//!
//! Six adjacent bands on a hearing scale (none of the sound left out, as the three bands before left out 150-300 Hz
//! and 2.5-5 kHz): sub (below 60 Hz), bass (60-150), low-mid (150-400), mid (400 Hz-2 kHz), presence (2-5 kHz) and air
//! (above 5 kHz). Two ways of listening:
//! - in time, through each band's filters (24 dB an octave at each edge): its level, and its crest factor (peak against
//!   RMS: punch, against density);
//! - in frequency, a short-time Fourier transform of the sound's middle and side (46 ms windows every 11.6 ms): each
//!   band's spectral flux (how much it changes: the onset strength, Bello et al. 2005) and the hits picked from it
//!   against a moving median; its width (side against middle); the percussive and the harmonic part of each band
//!   (Fitzgerald 2010: medians across time and across frequency of a log-frequency spectrogram, with soft masks) and
//!   the percussive part's hits; the spectral centroid; chroma, and the harmonic change (Harte et al. 2006: the tonal
//!   centroid's movement); the sections' novelty (Foote 2000: a checkerboard kernel along a beat-by-beat self-similarity
//!   of chroma and timbre) and their boundaries; and each beat's place in its bar.
//! - over seconds: swells (the loudness, overall and in each band, rising steadily for at least 1.5 s by more than a
//!   few dB: crescendos, risers, build-ups; a single hit is no swell), and build-ups (a swell that brightens and leads
//!   into a section boundary: as the whole track is known, the show can build towards the drop).
//!
//! The loudness as heard (EBU R128 momentary) comes from the analysis' own loudness meter and stays absolute: the show's
//! intensity follows it; the bands' signals are each against that band's own loudest, inside it.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};

/// The kept form's first byte. 1 and 2 (three bands): read as not worked out.
pub const FORMAT: u8 = 3;
/// The bands, low to high.
pub const BANDS: usize = 6;
/// Each band's edges (Hz), low to high; 0: open.
pub const EDGES: [(u16, u16); BANDS] = [(0, 60), (60, 150), (150, 400), (400, 2000), (2000, 5000), (5000, 0)];
/// The loudness as heard: values a second, and its scale (a byte is a third of a dB from -70 LUFS: 255 is +15).
pub const MOMENTS_RATE: usize = 10;
const MOMENT_FLOOR: f32 = -70.0;
const MOMENT_STEPS: f32 = 3.0;

/// The sections, by id.
pub mod id {
    pub const BAND_TABLE: u8 = 0;
    pub const LOUDNESS: u8 = 1;
    pub const LEVEL: u8 = 2;
    pub const FLUX: u8 = 3;
    pub const HITS: u8 = 4;
    pub const PERCUSSIVE: u8 = 5;
    pub const HARMONIC: u8 = 6;
    pub const PERCUSSIVE_HITS: u8 = 7;
    pub const CREST: u8 = 8;
    pub const WIDTH: u8 = 9;
    pub const CENTROID: u8 = 10;
    pub const CHROMA: u8 = 11;
    pub const HARMONIC_CHANGE: u8 = 12;
    pub const HARMONIC_CHANGES: u8 = 13;
    pub const NOVELTY: u8 = 14;
    pub const BOUNDARIES: u8 = 15;
    pub const BAR: u8 = 16;
    pub const SWELL: u8 = 17;
    pub const BAND_SWELL: u8 = 18;
    pub const BUILD_UP: u8 = 19;
}

/// What a section holds.
pub mod kind {
    /// Frames at a rate (0: one a beat of the track's beats), each a byte a channel.
    pub const SERIES: u8 = 0;
    /// Each channel's list of hits: time (ms) and strength.
    pub const HITS: u8 = 1;
    /// One list of marks: time (ms) and a value.
    pub const MARKS: u8 = 2;
    /// The band table: each band's low and high edge (Hz, 0 open).
    pub const TABLE: u8 = 3;
}

/// How a series' bytes read.
pub mod scale {
    /// 0..=255 for 0..1.
    pub const LINEAR: u8 = 0;
    /// dB over the 30 under the band's loudest (its 98th percentile).
    pub const DB_UNDER_TOP: u8 = 1;
    /// LUFS in thirds of a dB from -70.
    pub const LUFS_THIRDS: u8 = 2;
    /// log2 of the frequency, 20 Hz to 20 kHz.
    pub const LOG2_HZ: u8 = 3;
    /// Each frame against its own largest value.
    pub const PER_FRAME: u8 = 4;
    /// Against the track's own 98th percentile of it.
    pub const AGAINST_TOP: u8 = 5;
    /// dB, 0 to 30.
    pub const DB_0_30: u8 = 6;
}

/// Values a second of the series (Hz).
const LEVEL_RATE: usize = 20;
const FLUX_RATE: usize = 20;
const PARTS_RATE: usize = 10;
const SHAPE_RATE: usize = 5;
const CENTROID_RATE: usize = 10;
const CHROMA_RATE: usize = 5;
const TENTHS: usize = 10;
/// Novelty a second where a track has no beats to measure it by.
const UNBEATEN_RATE: usize = 2;

/// The filters' frames: energy every 10 ms.
const FRAMES_A_SECOND: usize = 100;
/// A band's level is kept over this many dB under its loudest.
const RANGE_DB: f32 = 30.0;
/// The transform's window, about 46 ms at any rate (a power of two), and four frames a window.
const WINDOW_SECS: f64 = 0.0464;
/// Magnitudes compressed as ln(1 + γ·|X|) before the flux (|X| 1 for a full-scale sine).
const GAMMA: f32 = 100.0;
/// The log-frequency spectrogram the parts are separated on: bins from 20 Hz to the top.
const LOG_BINS: usize = 96;
const LOG_FROM: f64 = 20.0;
/// Medians over 17 frames (about 200 ms) and over 17 log bins (about two thirds of an octave).
const HALF: usize = 8;
/// Hits closer than this to the last one in their band are one (ms): a kick rings longer than a hat.
const GAP_MS: [u32; BANDS] = [100, 100, 80, 70, 60, 50];
/// Mel bands for the timbre (MFCC), and the coefficients kept (the first, the level, left out).
const MELS: usize = 40;
const MFCC: usize = 13;
/// Units on either side in the novelty's kernel (beats, or half seconds).
const KERNEL: usize = 8;
/// Below this many beats, no bar position.
const BAR_BEATS: usize = 16;

const NONE: u8 = u8::MAX;

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

    /// Transposed direct form II.
    fn run(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// One band's filters in a row: two high-passes at its low edge, two low-passes at its high edge (24 dB an octave);
/// none at an open edge, or at an edge above what the rate holds.
fn chain(rate: u32, (low, high): (u16, u16)) -> Vec<Biquad> {
    let mut out = Vec::new();
    if low > 0 {
        out.extend([Biquad::new(rate, f64::from(low), true), Biquad::new(rate, f64::from(low), true)]);
    }
    if high > 0 && f64::from(high) < 0.45 * f64::from(rate) {
        out.extend([Biquad::new(rate, f64::from(high), false), Biquad::new(rate, f64::from(high), false)]);
    }
    out
}

/// Per frame of the transform.
#[derive(Clone, Copy, Default)]
struct Frame {
    /// Each band's middle and side energy (for its width), and its flux.
    mid: [f32; BANDS],
    side: [f32; BANDS],
    flux: [f32; BANDS],
    /// The spectral centroid (Hz; 0 in silence).
    centroid: f32,
    /// Each band's percussive and harmonic energy, and the percussive part's flux (filled 8 frames on).
    percussive: [f32; BANDS],
    harmonic: [f32; BANDS],
    percussive_flux: [f32; BANDS],
}

/// Per 10 ms of the filters.
#[derive(Clone, Copy, Default)]
struct Slice {
    /// Each band's mean square, and its largest sample.
    energy: [f32; BANDS],
    peak: [f32; BANDS],
}

/// Where each bin of the transform goes.
struct Bins {
    /// Its frequency (Hz).
    freq: Vec<f32>,
    /// Its band (NONE for the DC bin).
    band: Vec<u8>,
    /// Its log bin (NONE below 20 Hz).
    log: Vec<u8>,
    /// Each log bin's band (by its centre).
    log_band: Vec<u8>,
    /// Each log bin without a bin of its own (below a few hundred Hz the log bins are narrower than the transform's):
    /// the bin nearest its centre.
    log_fill: Vec<Option<usize>>,
    /// Its pitch class (C = 0; NONE outside 55 Hz to 5 kHz).
    pitch: Vec<u8>,
    /// Each mel band's bins and weights (triangles, 30 Hz to 8 kHz).
    mel: Vec<Vec<(usize, f32)>>,
}

impl Bins {
    fn new(rate: u32, n: usize) -> Self {
        let bins = n / 2 + 1;
        let rate = f64::from(rate);
        let freq: Vec<f32> = (0..bins).map(|k| (k as f64 * rate / n as f64) as f32).collect();
        let band_of = |f: f64| {
            EDGES.iter().position(|&(lo, hi)| f >= f64::from(lo) && (hi == 0 || f < f64::from(hi))).map_or(NONE, |b| b as u8)
        };
        let band = freq.iter().enumerate().map(|(k, f)| if k == 0 { NONE } else { band_of(f64::from(*f)) }).collect();
        let top = (rate / 2.0).min(20_000.0);
        let span = (top / LOG_FROM).ln();
        let log_index = |f: f64| -> Option<usize> { (f >= LOG_FROM && f < top).then(|| (((f / LOG_FROM).ln() / span) * LOG_BINS as f64) as usize).map(|i| i.min(LOG_BINS - 1)) };
        let log: Vec<u8> = freq.iter().map(|f| log_index(f64::from(*f)).map_or(NONE, |i| i as u8)).collect();
        let centre = |i: usize| LOG_FROM * (span * (i as f64 + 0.5) / LOG_BINS as f64).exp();
        let log_band = (0..LOG_BINS).map(|i| band_of(centre(i))).collect();
        let log_fill = (0..LOG_BINS)
            .map(|i| (!log.contains(&(i as u8))).then(|| ((centre(i) * n as f64 / rate).round() as usize).clamp(1, bins - 1)))
            .collect();
        let pitch = freq
            .iter()
            .map(|f| {
                let f = f64::from(*f);
                if (55.0..=5000.0).contains(&f) { ((12.0 * (f / 440.0).log2()).round() as i64 + 9).rem_euclid(12) as u8 } else { NONE }
            })
            .collect();
        let mel_of = |f: f64| 2595.0 * (1.0 + f / 700.0).log10();
        let hz_of = |m: f64| 700.0 * (10f64.powf(m / 2595.0) - 1.0);
        let (lo, hi) = (mel_of(30.0), mel_of(8000f64.min(rate / 2.0 * 0.95)));
        let points: Vec<f64> = (0..MELS + 2).map(|i| hz_of(lo + (hi - lo) * i as f64 / (MELS + 1) as f64)).collect();
        let mel = (0..MELS)
            .map(|m| {
                let (a, b, c) = (points[m], points[m + 1], points[m + 2]);
                (1..bins)
                    .filter_map(|k| {
                        let f = f64::from(freq[k]);
                        let w = if f > a && f <= b { (f - a) / (b - a) } else if f > b && f < c { (c - f) / (c - b) } else { 0.0 };
                        (w > 0.0).then_some((k, w as f32))
                    })
                    .collect()
            })
            .collect();
        Bins { freq, band, log, log_band, log_fill, pitch, mel }
    }
}

/// Above this rate the sound is taken down to it first (2 or 4 to 1, after two low-pass filters at 20 kHz): nothing the
/// show is steered by lies above 20 kHz, and a 192 kHz file took five times as long a minute as a CD's (NAS,
/// 2026-10-18).
const RATE_ABOVE: u32 = 48_000;

/// Listens to a track for its signals: the sound in (each sample's middle, the channels' mean, and side, half the
/// first two channels' difference), its signals out (`finish`).
pub struct SignalEar {
    /// The rate it listens at (the file's, or a part of it: `RATE_ABOVE`).
    rate: u32,
    samples: u64,
    // Taking a high rate down: keep one sample in `down`, after the low-passes (middle, side).
    down: usize,
    skipped: usize,
    down_filters: [[Biquad; 2]; 2],
    // The filters, a chain a band, and their 10 ms slices.
    filters: Vec<Vec<Biquad>>,
    slice_len: usize,
    in_slice: usize,
    slice: Slice,
    slices: Vec<Slice>,
    // The transform: a window of `n` samples every `hop`, the middle's and the side's.
    n: usize,
    hop: usize,
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    mid: Vec<f32>,
    side: Vec<f32>,
    at: usize,
    since: usize,
    input: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    power: Vec<f32>,
    bins: Bins,
    previous: Vec<f32>,
    frames: Vec<Frame>,
    // The parts: the log-frequency spectra of the frames around the one separated next.
    logs: VecDeque<Vec<f32>>,
    separated: usize,
    previous_percussive: Vec<f32>,
    // Chroma and timbre, ten a second.
    tenth: usize,
    in_tenth: usize,
    chroma_sum: [f32; 12],
    mfcc_sum: [f32; MFCC],
    chroma: Vec<[f32; 12]>,
    mfcc: Vec<[f32; MFCC]>,
}

impl SignalEar {
    pub fn new(file_rate: u32) -> Self {
        let down = if file_rate > RATE_ABOVE { (file_rate as f64 / f64::from(RATE_ABOVE)).ceil() as usize } else { 1 };
        let lowpass = || [Biquad::new(file_rate, 20_000.0, false), Biquad::new(file_rate, 20_000.0, false)];
        let rate = file_rate / down as u32;
        let n = (f64::from(rate) * WINDOW_SECS).round().max(256.0) as usize;
        let n = n.next_power_of_two();
        let hop = n / 4;
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(n);
        let spectrum = fft.make_output_vec();
        let scratch = fft.make_scratch_vec();
        // Hann, scaled so a full-scale sine's bin is about 1.
        let window: Vec<f32> = (0..n).map(|i| (0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos()) as f32 * 4.0 / n as f32).collect();
        SignalEar {
            rate,
            samples: 0,
            down,
            skipped: 0,
            down_filters: [lowpass(), lowpass()],
            filters: EDGES.iter().map(|&e| chain(rate, e)).collect(),
            slice_len: (rate as usize / FRAMES_A_SECOND).max(1),
            in_slice: 0,
            slice: Slice::default(),
            slices: Vec::new(),
            n,
            hop,
            power: vec![0.0; n / 2 + 1],
            previous: vec![0.0; n / 2 + 1],
            bins: Bins::new(rate, n),
            fft,
            window,
            mid: vec![0.0; n],
            side: vec![0.0; n],
            at: 0,
            since: 0,
            input: vec![0.0; n],
            spectrum,
            scratch,
            frames: Vec::new(),
            logs: VecDeque::with_capacity(2 * HALF + 1),
            separated: 0,
            previous_percussive: vec![0.0; LOG_BINS],
            tenth: 0,
            in_tenth: 0,
            chroma_sum: [0.0; 12],
            mfcc_sum: [0.0; MFCC],
            chroma: Vec::new(),
            mfcc: Vec::new(),
        }
    }

    /// One sample: its middle (the channels' mean) and side (half the first two channels' difference; 0 for mono).
    pub fn hear(&mut self, mid: f32, side: f32) {
        let (mid, side) = if self.down > 1 {
            let [m, s] = &mut self.down_filters;
            let mid = m.iter_mut().fold(f64::from(mid), |v, f| f.run(v)) as f32;
            let side = s.iter_mut().fold(f64::from(side), |v, f| f.run(v)) as f32;
            self.skipped += 1;
            if self.skipped < self.down {
                return;
            }
            self.skipped = 0;
            (mid, side)
        } else {
            (mid, side)
        };
        self.samples += 1;
        let x = f64::from(mid);
        for (b, chain) in self.filters.iter_mut().enumerate() {
            let y = chain.iter_mut().fold(x, |v, f| f.run(v)) as f32;
            self.slice.energy[b] += y * y;
            self.slice.peak[b] = self.slice.peak[b].max(y.abs());
        }
        self.in_slice += 1;
        if self.in_slice == self.slice_len {
            let len = self.slice_len as f32;
            for e in self.slice.energy.iter_mut() {
                *e /= len;
            }
            self.slices.push(self.slice);
            (self.slice, self.in_slice) = (Slice::default(), 0);
        }
        self.mid[self.at] = mid;
        self.side[self.at] = side;
        self.at = (self.at + 1) % self.n;
        self.since += 1;
        if self.since == self.hop {
            self.since = 0;
            self.frame();
        }
    }

    /// Frame `k`'s window starts this many samples in (before the first sample: silence).
    fn start(&self, k: usize) -> f64 {
        ((k + 1) * self.hop) as f64 - self.n as f64
    }

    /// Frame `k`'s middle (s).
    fn centre(&self, k: usize) -> f64 {
        (self.start(k) + self.n as f64 / 2.0) / f64::from(self.rate)
    }

    /// Where a sudden start lies when frame `k`'s flux is the one that peaks on it (s): about seven tenths into its
    /// window (the log compression makes the first frame it shows in the largest rise; measured on clicks).
    fn onset(&self, k: usize) -> f64 {
        (self.start(k) + ONSET_INTO * self.n as f64) / f64::from(self.rate)
    }

    /// The transform of the window that ends now, the middle's and the side's.
    fn frame(&mut self) {
        let n = self.n;
        for i in 0..n {
            self.input[i] = self.mid[(self.at + i) % n] * self.window[i];
        }
        let _ = self.fft.process_with_scratch(&mut self.input, &mut self.spectrum, &mut self.scratch);
        for (p, c) in self.power.iter_mut().zip(&self.spectrum) {
            *p = c.norm_sqr();
        }
        for i in 0..n {
            self.input[i] = self.side[(self.at + i) % n] * self.window[i];
        }
        let _ = self.fft.process_with_scratch(&mut self.input, &mut self.spectrum, &mut self.scratch);
        let mut f = Frame::default();
        let (mut weighted, mut total) = (0f64, 0f64);
        let mut chroma = [0f32; 12];
        let mut log = vec![0f32; LOG_BINS];
        for k in 1..self.power.len() {
            let p = self.power[k];
            let magnitude = p.sqrt();
            let y = (1.0 + GAMMA * magnitude).ln();
            let rise = (y - self.previous[k]).max(0.0);
            self.previous[k] = y;
            let b = self.bins.band[k];
            if b != NONE {
                let b = usize::from(b);
                f.mid[b] += p;
                f.side[b] += self.spectrum[k].norm_sqr();
                f.flux[b] += rise;
            }
            let hz = self.bins.freq[k];
            if hz >= 20.0 {
                weighted += f64::from(hz * magnitude);
                total += f64::from(magnitude);
            }
            let pc = self.bins.pitch[k];
            if pc != NONE {
                chroma[usize::from(pc)] += magnitude;
            }
            let l = self.bins.log[k];
            if l != NONE {
                log[usize::from(l)] += p;
            }
        }
        for (i, fill) in self.bins.log_fill.iter().enumerate() {
            if let Some(k) = fill {
                log[i] = self.power[*k];
            }
        }
        f.centroid = if total > 1e-9 { (weighted / total) as f32 } else { 0.0 };
        // Timbre: the mel bands' log energies, through a cosine transform.
        let mels: Vec<f32> = self.bins.mel.iter().map(|m| (m.iter().map(|&(k, w)| self.power[k] * w).sum::<f32>() + 1e-10).log10()).collect();
        let mut mfcc = [0f32; MFCC];
        for (j, c) in mfcc.iter_mut().enumerate() {
            let j = j + 1;
            *c = mels.iter().enumerate().map(|(m, v)| v * (std::f32::consts::PI * j as f32 * (m as f32 + 0.5) / MELS as f32).cos()).sum();
        }
        let k = self.frames.len();
        self.frames.push(f);
        self.tenths(k, &chroma, &mfcc);
        self.logs.push_back(log);
        if self.logs.len() > 2 * HALF + 1 {
            self.logs.pop_front();
        }
        while self.separated + HALF <= k {
            self.separate(self.separated);
            self.separated += 1;
        }
    }

    /// Chroma and timbre summed into tenths of a second, by each frame's middle.
    fn tenths(&mut self, k: usize, chroma: &[f32; 12], mfcc: &[f32; MFCC]) {
        let tenth = (self.centre(k).max(0.0) * TENTHS as f64) as usize;
        while tenth > self.tenth {
            self.close_tenth();
        }
        for (s, c) in self.chroma_sum.iter_mut().zip(chroma) {
            *s += c;
        }
        for (s, c) in self.mfcc_sum.iter_mut().zip(mfcc) {
            *s += c;
        }
        self.in_tenth += 1;
    }

    fn close_tenth(&mut self) {
        let n = self.in_tenth.max(1) as f32;
        self.chroma.push(self.chroma_sum.map(|c| c / n));
        self.mfcc.push(self.mfcc_sum.map(|c| c / n));
        (self.chroma_sum, self.mfcc_sum, self.in_tenth) = ([0.0; 12], [0.0; MFCC], 0);
        self.tenth += 1;
    }

    /// Frame `j`'s percussive and harmonic parts: its harmonic energy is the median of each log bin across the frames
    /// around it, its percussive energy the median across the bins around each; each bin's energy is shared between
    /// them by those (soft masks: P/(P+H) on energies, Wiener's on magnitudes).
    fn separate(&mut self, j: usize) {
        let oldest = self.frames.len() - self.logs.len();
        let (lo, hi) = (j.saturating_sub(HALF).max(oldest), (j + HALF).min(self.frames.len() - 1));
        let mut across = [0f32; 2 * HALF + 1];
        let (mut percussive, mut harmonic, mut flux) = ([0f32; BANDS], [0f32; BANDS], [0f32; BANDS]);
        let x = &self.logs[j - oldest];
        for bin in 0..LOG_BINS {
            let m = hi - lo + 1;
            for (t, i) in (lo..=hi).enumerate() {
                across[t] = self.logs[i - oldest][bin];
            }
            let h = median(&mut across[..m]);
            let (flo, fhi) = (bin.saturating_sub(HALF), (bin + HALF).min(LOG_BINS - 1));
            let mut down = [0f32; 2 * HALF + 1];
            down[..fhi - flo + 1].copy_from_slice(&x[flo..=fhi]);
            let p = median(&mut down[..fhi - flo + 1]);
            let mask = if p + h > 0.0 { p / (p + h) } else { 0.5 };
            let (pe, he) = (x[bin] * mask, x[bin] * (1.0 - mask));
            let y = (1.0 + GAMMA * pe.sqrt()).ln();
            let rise = (y - self.previous_percussive[bin]).max(0.0);
            self.previous_percussive[bin] = y;
            let b = self.bins.log_band[bin];
            if b != NONE {
                let b = usize::from(b);
                percussive[b] += pe;
                harmonic[b] += he;
                flux[b] += rise;
            }
        }
        let f = &mut self.frames[j];
        (f.percussive, f.harmonic, f.percussive_flux) = (percussive, harmonic, flux);
    }

    /// The signals, with `moments`: the momentary loudness (LUFS) of each 400 ms window ending every 100 ms from the
    /// fourth on; `beats`: the track's beats (s), as the analysis keeps them; and `downbeats`, where Beat this! heard
    /// them (decision 0049, amended), each at one of the beats.
    pub fn finish(mut self, moments: &[f32], beats: &[f64], downbeats: Option<&[f64]>) -> Signals {
        while self.separated < self.frames.len() {
            self.separate(self.separated);
            self.separated += 1;
        }
        if self.in_tenth > 0 {
            self.close_tenth();
        }
        if self.in_slice > 0 {
            let len = self.in_slice as f32;
            for e in self.slice.energy.iter_mut() {
                *e /= len;
            }
            self.slices.push(self.slice);
        }
        let duration = self.samples as f64 / f64::from(self.rate);
        let mut out = Writer::new();

        // 0: the bands.
        let mut table = Vec::new();
        for (lo, hi) in EDGES {
            table.extend_from_slice(&lo.to_le_bytes());
            table.extend_from_slice(&hi.to_le_bytes());
        }
        out.section(id::BAND_TABLE, kind::TABLE, 0, BANDS as u8, BANDS as u32, scale::LINEAR, &table);

        // 1: the loudness as heard, each window by its middle: the first ends 400 ms in, around 200 ms; the two before
        // it are its value too.
        let centred: Vec<f32> = moments.first().map(|m| [*m, *m]).into_iter().flatten().chain(moments.iter().copied()).collect();
        let bytes: Vec<u8> = centred.iter().map(|m| if m.is_finite() { ((m - MOMENT_FLOOR) * MOMENT_STEPS).clamp(0.0, 255.0).round() as u8 } else { 0 }).collect();
        out.series(id::LOUDNESS, MOMENTS_RATE, 1, scale::LUFS_THIRDS, &bytes);

        // 2: the bands' levels, 20 a second, from the filters: each band's loudest is the 98th percentile of these 50 ms
        // levels (a 10 ms slice is shorter than a low tone's cycle).
        let slice_centre = |i: usize| (i as f64 + 0.5) / FRAMES_A_SECOND as f64;
        let count = frames_in(duration, LEVEL_RATE);
        let mut sums = vec![([0f32; BANDS], 0u32); count];
        for (i, s) in self.slices.iter().enumerate() {
            let at = bucket(slice_centre(i), LEVEL_RATE, count);
            for b in 0..BANDS {
                sums[at].0[b] += s.energy[b];
            }
            sums[at].1 += 1;
        }
        let levels: Vec<[f32; BANDS]> = sums.iter().map(|(e, n)| e.map(|v| db(v / (*n).max(1) as f32))).collect();
        let tops: Vec<f32> = (0..BANDS).map(|b| percentile(levels.iter().map(|l| l[b]).filter(|v| *v > -100.0).collect(), 0.98).unwrap_or(-100.0)).collect();
        let bytes: Vec<u8> = levels
            .iter()
            .flat_map(|l| (0..BANDS).map(|b| if l[b] > -100.0 { (((l[b] - (tops[b] - RANGE_DB)) / RANGE_DB).clamp(0.0, 1.0) * 255.0).round() as u8 } else { 0 }).collect::<Vec<_>>())
            .collect();
        out.series(id::LEVEL, LEVEL_RATE, BANDS, scale::DB_UNDER_TOP, &bytes);

        // 3: flux, 20 a second: the largest in each 50 ms, against the band's 98th percentile.
        let flux_tops: Vec<f32> = (0..BANDS).map(|b| top_of(self.frames.iter().map(|f| f.flux[b]))).collect();
        let count = frames_in(duration, FLUX_RATE);
        let mut most = vec![[0f32; BANDS]; count];
        for (k, f) in self.frames.iter().enumerate() {
            let at = bucket(self.centre(k), FLUX_RATE, count);
            for b in 0..BANDS {
                most[at][b] = most[at][b].max(f.flux[b]);
            }
        }
        let bytes: Vec<u8> = most.iter().flat_map(|m| (0..BANDS).map(|b| against(m[b], flux_tops[b])).collect::<Vec<_>>()).collect();
        out.series(id::FLUX, FLUX_RATE, BANDS, scale::AGAINST_TOP, &bytes);

        // 4: hits, from the flux.
        let onsets: Vec<f64> = (0..self.frames.len()).map(|k| self.onset(k)).collect();
        let hits: Vec<Vec<(u32, u8)>> = (0..BANDS).map(|b| pick(&self.frames.iter().map(|f| f.flux[b]).collect::<Vec<_>>(), &onsets, GAP_MS[b], self.hop, self.rate)).collect();
        out.hits(id::HITS, &hits);

        // 5, 6: the percussive and the harmonic part's level, 10 a second, on the band's own scale (its loudest in the
        // transform, so the two parts add up to its level there).
        let frame_tops: Vec<f32> = (0..BANDS)
            .map(|b| percentile(self.frames.iter().map(|f| db(f.mid[b])).filter(|v| *v > -100.0).collect(), 0.98).unwrap_or(-100.0))
            .collect();
        let part_of = |energy: f32, b: usize| {
            let level = db(energy);
            if level > -100.0 { (((level - (frame_tops[b] - RANGE_DB)) / RANGE_DB).clamp(0.0, 1.0) * 255.0).round() as u8 } else { 0 }
        };
        let count = frames_in(duration, PARTS_RATE);
        let mut parts = vec![([0f32; BANDS], [0f32; BANDS], 0u32); count];
        for (k, f) in self.frames.iter().enumerate() {
            let at = bucket(self.centre(k), PARTS_RATE, count);
            for b in 0..BANDS {
                parts[at].0[b] += f.percussive[b];
                parts[at].1[b] += f.harmonic[b];
            }
            parts[at].2 += 1;
        }
        let (mut percussive, mut harmonic) = (Vec::new(), Vec::new());
        for (p, h, n) in &parts {
            let n = (*n).max(1) as f32;
            for b in 0..BANDS {
                percussive.push(part_of(p[b] / n, b));
                harmonic.push(part_of(h[b] / n, b));
            }
        }
        out.series(id::PERCUSSIVE, PARTS_RATE, BANDS, scale::DB_UNDER_TOP, &percussive);
        out.series(id::HARMONIC, PARTS_RATE, BANDS, scale::DB_UNDER_TOP, &harmonic);

        // 7: the percussive part's hits.
        let hits: Vec<Vec<(u32, u8)>> =
            (0..BANDS).map(|b| pick(&self.frames.iter().map(|f| f.percussive_flux[b]).collect::<Vec<_>>(), &onsets, GAP_MS[b], self.hop, self.rate)).collect();
        out.hits(id::PERCUSSIVE_HITS, &hits);

        // 8: crest, 5 a second: each 200 ms's peak against its RMS (dB, 0 to 30), from the filters.
        let count = frames_in(duration, SHAPE_RATE);
        let mut shape = vec![([0f32; BANDS], [0f32; BANDS], 0u32); count];
        for (i, s) in self.slices.iter().enumerate() {
            let at = bucket(slice_centre(i), SHAPE_RATE, count);
            for b in 0..BANDS {
                shape[at].0[b] = shape[at].0[b].max(s.peak[b]);
                shape[at].1[b] += s.energy[b];
            }
            shape[at].2 += 1;
        }
        let bytes: Vec<u8> = shape
            .iter()
            .flat_map(|(peak, energy, n)| {
                (0..BANDS).map(move |b| {
                    let rms = (energy[b] / (*n).max(1) as f32).sqrt();
                    if rms > 1e-5 { ((20.0 * (peak[b] / rms).log10()).clamp(0.0, 30.0) / 30.0 * 255.0).round() as u8 } else { 0 }
                })
            })
            .collect();
        out.series(id::CREST, SHAPE_RATE, BANDS, scale::DB_0_30, &bytes);

        // 9: width, 5 a second: each band's side energy against its middle's and side's (0 mono, 0.5 two unrelated
        // channels, 1 one channel the other's opposite).
        let mut wide = vec![([0f32; BANDS], [0f32; BANDS]); count];
        for (k, f) in self.frames.iter().enumerate() {
            let at = bucket(self.centre(k), SHAPE_RATE, count);
            for b in 0..BANDS {
                wide[at].0[b] += f.mid[b];
                wide[at].1[b] += f.side[b];
            }
        }
        let bytes: Vec<u8> = wide
            .iter()
            .flat_map(|(m, s)| (0..BANDS).map(move |b| if m[b] + s[b] > 1e-12 { (s[b] / (m[b] + s[b]) * 255.0).round() as u8 } else { 0 }))
            .collect();
        out.series(id::WIDTH, SHAPE_RATE, BANDS, scale::LINEAR, &bytes);

        // 10: the spectral centroid, 10 a second: log2 of 20 Hz to 20 kHz.
        let count = frames_in(duration, CENTROID_RATE);
        let mut centroids = vec![(0f64, 0u32); count];
        for (k, f) in self.frames.iter().enumerate() {
            if f.centroid > 0.0 {
                let at = bucket(self.centre(k), CENTROID_RATE, count);
                centroids[at].0 += f64::from(f.centroid);
                centroids[at].1 += 1;
            }
        }
        let bytes: Vec<u8> = centroids
            .iter()
            .map(|(sum, n)| if *n > 0 { (((sum / f64::from(*n) / 20.0).log2() / 1000f64.log2()).clamp(0.0, 1.0) * 255.0).round() as u8 } else { 0 })
            .collect();
        out.series(id::CENTROID, CENTROID_RATE, 1, scale::LOG2_HZ, &bytes);

        // 11: chroma, 5 a second, each frame against its own strongest class.
        let tenths = frames_in(duration, TENTHS);
        self.chroma.resize(tenths, [0.0; 12]);
        self.mfcc.resize(tenths, [0.0; MFCC]);
        let per = TENTHS / CHROMA_RATE;
        let bytes: Vec<u8> = self
            .chroma
            .chunks(per)
            .flat_map(|c| {
                let mut sum = [0f32; 12];
                for x in c {
                    for (s, v) in sum.iter_mut().zip(x) {
                        *s += v;
                    }
                }
                let top = sum.iter().copied().fold(0f32, f32::max);
                sum.map(|v| if top > 1e-9 { (v / top * 255.0).round() as u8 } else { 0 })
            })
            .collect();
        out.series(id::CHROMA, CHROMA_RATE, 12, scale::PER_FRAME, &bytes);

        // 12, 13: the harmonic change, 10 a second, and its peaks.
        let change = harmonic_change(&self.chroma);
        let top = top_of(change.iter().copied());
        out.series(id::HARMONIC_CHANGE, TENTHS, 1, scale::AGAINST_TOP, &change.iter().map(|v| against(*v, top)).collect::<Vec<_>>());
        let marks: Vec<(u32, u8)> = peaks(&change, 5, 0.0).into_iter().map(|i| ((i * 100) as u32, against(change[i], top))).collect();
        out.marks(id::HARMONIC_CHANGES, &marks);

        // 14, 15: the sections' novelty, a value a beat (or two a second without beats), and their boundaries.
        let beaten = beats.len() >= 2 * KERNEL;
        let units: Vec<f64> = if beaten { beats.to_vec() } else { (0..frames_in(duration, UNBEATEN_RATE)).map(|i| i as f64 / UNBEATEN_RATE as f64).collect() };
        let novelty = novelty(&units, duration, &self.chroma, &self.mfcc);
        let top = top_of(novelty.iter().copied());
        out.series(id::NOVELTY, if beaten { 0 } else { UNBEATEN_RATE }, 1, scale::AGAINST_TOP, &novelty.iter().map(|v| against(*v, top)).collect::<Vec<_>>());
        let largest = novelty.iter().copied().fold(0f32, f32::max);
        let marks: Vec<(u32, u8)> =
            peaks(&novelty, KERNEL, 0.4 * largest).into_iter().map(|i| ((units[i] * 1000.0).round() as u32, against(novelty[i], top))).collect();
        out.marks(id::BOUNDARIES, &marks);

        // 16: each beat's place in its bar: from Beat this!'s downbeats where it heard some (sure), else by where the
        // low bands' starts fall hardest.
        if let Some(downbeats) = downbeats.filter(|d| !d.is_empty() && !beats.is_empty()) {
            out.series(id::BAR, 0, 1, 255, &places(beats, downbeats));
        } else if beats.len() >= BAR_BEATS {
            let low: Vec<f32> = self.frames.iter().map(|f| f.flux[0] + f.flux[1]).collect();
            let at_beat: Vec<f32> = beats
                .iter()
                .map(|t| {
                    let near = |k: &usize| (onsets[*k] - t).abs() <= 0.035;
                    let first = onsets.partition_point(|o| *o < t - 0.035);
                    (first..onsets.len()).take_while(|k| onsets[*k] <= t + 0.035).filter(near).map(|k| low[k]).fold(0f32, f32::max)
                })
                .collect();
            let (meter, phase, sure) = bar(&at_beat);
            let bytes: Vec<u8> = (0..beats.len()).map(|i| ((i + meter - phase % meter) % meter) as u8).collect();
            out.series(id::BAR, 0, 1, (sure.clamp(0.0, 1.0) * 255.0).round() as u8, &bytes);
        }

        // 17: the swell, 10 a second, from the loudness as heard.
        let mut loud: Vec<f32> = centred.iter().map(|m| if m.is_finite() { m.max(MOMENT_FLOOR) } else { MOMENT_FLOOR }).collect();
        loud.resize(tenths, loud.last().copied().unwrap_or(MOMENT_FLOOR));
        let overall = swell(&loud);
        out.series(id::SWELL, TENTHS, 1, scale::LINEAR, &overall.iter().map(|v| (v * 255.0).round() as u8).collect::<Vec<_>>());

        // 18: each band's swell, from its level in tenths.
        let mut band_db = vec![[0f32; BANDS]; tenths];
        let mut counts = vec![0u32; tenths];
        for (i, s) in self.slices.iter().enumerate() {
            let at = bucket(slice_centre(i), TENTHS, tenths);
            for b in 0..BANDS {
                band_db[at][b] += s.energy[b];
            }
            counts[at] += 1;
        }
        let per_band: Vec<Vec<f32>> = (0..BANDS)
            .map(|b| swell(&band_db.iter().zip(&counts).map(|(e, n)| db(e[b] / (*n).max(1) as f32).max(-100.0)).collect::<Vec<_>>()))
            .collect();
        let bytes: Vec<u8> = (0..tenths).flat_map(|i| per_band.iter().map(move |s| (s[i] * 255.0).round() as u8).collect::<Vec<_>>()).collect();
        out.series(id::BAND_SWELL, TENTHS, BANDS, scale::LINEAR, &bytes);

        // 19: build-ups: the swell, more where the sound brightens with it and where a section boundary follows soon.
        let bright: Vec<f32> = centroids.iter().map(|(sum, n)| if *n > 0 { (sum / f64::from(*n) / 20.0).log2() as f32 } else { 0.0 }).collect();
        let boundaries: Vec<f64> = marks.iter().map(|(ms, _)| f64::from(*ms) / 1000.0).collect();
        let build: Vec<f32> = (0..tenths)
            .map(|i| {
                if overall[i] <= 0.0 {
                    return 0.0;
                }
                let t = i as f64 / TENTHS as f64;
                // Brightening: the centroid now against 3 s before (half an octave up is all of it).
                let before = bright.get(i.saturating_sub(30)).copied().unwrap_or(0.0);
                let now = bright.get(i).copied().unwrap_or(0.0);
                let brightening = if before > 0.0 && now > 0.0 { ((now - before) / 0.5).clamp(0.0, 1.0) } else { 0.0 };
                // A boundary within 4 s ahead counts fully, fading out by 10 s.
                let ahead = boundaries.iter().find(|b| **b > t).map_or(0.0, |b| (1.0 - ((b - t - 4.0) / 6.0).clamp(0.0, 1.0)) as f32);
                overall[i] * (0.5 + 0.25 * brightening + 0.25 * ahead)
            })
            .collect();
        out.series(id::BUILD_UP, TENTHS, 1, scale::LINEAR, &build.iter().map(|v| (v * 255.0).round() as u8).collect::<Vec<_>>());
        out.done()
    }
}

/// A swell at each tenth of a second (0..1) from a level in dB ten a second: the level (its median over 0.7 s, so a
/// single hit does not count) is rising now, by at least 2 dB over the last 1.5 s and mostly step by step, and has
/// risen more than 3 dB since the lowest it was in the 8 s before (all of it at 12 dB); none otherwise, a level that
/// rose and stays is no swell.
fn swell(level: &[f32]) -> Vec<f32> {
    let n = level.len();
    let mut around = Vec::with_capacity(7);
    let smooth: Vec<f32> = (0..n)
        .map(|i| {
            around.clear();
            around.extend_from_slice(&level[i.saturating_sub(3)..(i + 4).min(n)]);
            median(&mut around)
        })
        .collect();
    (0..n)
        .map(|i| {
            if i < 15 || smooth[i] - smooth[i - 15] < 2.0 {
                return 0.0;
            }
            let up = (i - 15..i).filter(|j| smooth[j + 1] >= smooth[*j]).count();
            if up < 10 {
                return 0.0;
            }
            let low = smooth[i.saturating_sub(80)..=i].iter().copied().fold(f32::MAX, f32::min);
            ((smooth[i] - low - 3.0) / 9.0).clamp(0.0, 1.0)
        })
        .collect()
}

/// Where the flux's peak frame puts a sudden start, as a share of its window from the window's start (measured on
/// clicks and drum-like bursts; see the tests).
const ONSET_INTO: f64 = 0.7;

/// A track's signals as kept and served (format 3): the format byte, the number of sections; then each section's
/// header (its id, kind, rate in tenths of Hz (0: a value a beat), channels, count, scale: 10 bytes, numbers
/// little-endian) and its bytes:
/// - a series: `count` frames of a byte a channel, frame after frame;
/// - hits: for each channel, the number of its hits (u32), then each one's time (ms, u32) and strength (a byte);
///   `count` is all channels' hits together;
/// - marks: `count` times (ms, u32), each with a byte;
/// - the band table: for each band, its low and high edge (Hz, u16; 0 open).
///
/// Empty where the decode did not cover the whole file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Signals(pub Vec<u8>);

/// A section, read back.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub kind: u8,
    /// Tenths of Hz; 0: a value a beat.
    pub rate: u16,
    pub channels: u8,
    pub scale: u8,
    pub data: Data,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Data {
    Series(Vec<u8>),
    Hits(Vec<Vec<(u32, u8)>>),
    Marks(Vec<(u32, u8)>),
    Table(Vec<(u16, u16)>),
}

impl Section {
    /// A series' frames, a slice of channels each.
    pub fn frames(&self) -> Vec<&[u8]> {
        match &self.data {
            Data::Series(b) => b.chunks(usize::from(self.channels).max(1)).collect(),
            _ => Vec::new(),
        }
    }
}

impl Signals {
    /// One section, read back.
    pub fn section(&self, id: u8) -> Option<Section> {
        self.read()?.remove(&id)
    }

    /// The loudness as heard (LUFS), `MOMENTS_RATE` a second.
    pub fn loudness(&self) -> Option<Vec<f32>> {
        match self.section(id::LOUDNESS)?.data {
            Data::Series(b) => Some(b.iter().map(|v| MOMENT_FLOOR + f32::from(*v) / MOMENT_STEPS).collect()),
            _ => None,
        }
    }

    /// A hits section's lists, a band each.
    pub fn hits(&self, id: u8) -> Option<Vec<Vec<(u32, u8)>>> {
        match self.section(id)?.data {
            Data::Hits(h) => Some(h),
            _ => None,
        }
    }

    /// Its sections by id; None for bytes not in this form.
    pub fn read(&self) -> Option<BTreeMap<u8, Section>> {
        let b = &self.0;
        if b.len() < 2 || b[0] != FORMAT {
            return None;
        }
        let u16_at = |i: usize| b.get(i..i + 2).map(|s| u16::from_le_bytes([s[0], s[1]]));
        let u32_at = |i: usize| b.get(i..i + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]));
        let mut out = BTreeMap::new();
        let mut at = 2;
        for _ in 0..b[1] {
            let (id, kind, rate, channels, count, scale) = (*b.get(at)?, *b.get(at + 1)?, u16_at(at + 2)?, *b.get(at + 4)?, u32_at(at + 5)? as usize, *b.get(at + 9)?);
            at += 10;
            let data = match kind {
                kind::SERIES => {
                    let len = count * usize::from(channels);
                    let d = b.get(at..at + len)?.to_vec();
                    at += len;
                    Data::Series(d)
                }
                kind::HITS => {
                    let mut lists = Vec::new();
                    for _ in 0..channels {
                        let n = u32_at(at)? as usize;
                        at += 4;
                        let mut list = Vec::with_capacity(n);
                        for _ in 0..n {
                            list.push((u32_at(at)?, *b.get(at + 4)?));
                            at += 5;
                        }
                        lists.push(list);
                    }
                    Data::Hits(lists)
                }
                kind::MARKS => {
                    let mut list = Vec::with_capacity(count);
                    for _ in 0..count {
                        list.push((u32_at(at)?, *b.get(at + 4)?));
                        at += 5;
                    }
                    Data::Marks(list)
                }
                kind::TABLE => {
                    let mut list = Vec::with_capacity(count);
                    for _ in 0..count {
                        list.push((u16_at(at)?, u16_at(at + 2)?));
                        at += 4;
                    }
                    Data::Table(list)
                }
                _ => return None,
            };
            out.insert(id, Section { kind, rate, channels, scale, data });
        }
        (at == b.len()).then_some(out)
    }
}

/// Builds the kept form, a section at a time.
struct Writer {
    body: Vec<u8>,
    sections: u8,
}

impl Writer {
    fn new() -> Self {
        Writer { body: Vec::new(), sections: 0 }
    }

    fn section(&mut self, id: u8, kind: u8, rate: usize, channels: u8, count: u32, scale: u8, payload: &[u8]) {
        self.body.push(id);
        self.body.push(kind);
        self.body.extend_from_slice(&((rate * 10).min(usize::from(u16::MAX)) as u16).to_le_bytes());
        self.body.push(channels);
        self.body.extend_from_slice(&count.to_le_bytes());
        self.body.push(scale);
        self.body.extend_from_slice(payload);
        self.sections += 1;
    }

    fn series(&mut self, id: u8, rate: usize, channels: usize, scale: u8, bytes: &[u8]) {
        let count = (bytes.len() / channels.max(1)) as u32;
        self.section(id, kind::SERIES, rate, channels as u8, count, scale, bytes);
    }

    fn hits(&mut self, id: u8, lists: &[Vec<(u32, u8)>]) {
        let mut payload = Vec::new();
        for list in lists {
            payload.extend_from_slice(&(list.len() as u32).to_le_bytes());
            for (ms, strength) in list {
                payload.extend_from_slice(&ms.to_le_bytes());
                payload.push(*strength);
            }
        }
        let count = lists.iter().map(Vec::len).sum::<usize>() as u32;
        self.section(id, kind::HITS, 0, lists.len() as u8, count, scale::LINEAR, &payload);
    }

    fn marks(&mut self, id: u8, list: &[(u32, u8)]) {
        let mut payload = Vec::new();
        for (ms, v) in list {
            payload.extend_from_slice(&ms.to_le_bytes());
            payload.push(*v);
        }
        self.section(id, kind::MARKS, 0, 1, list.len() as u32, scale::AGAINST_TOP, &payload);
    }

    fn done(self) -> Signals {
        let mut out = vec![FORMAT, self.sections];
        out.extend(self.body);
        Signals(out)
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

/// A signal's top: its 98th percentile over the values above zero (a tiny floor where there are none).
fn top_of(values: impl Iterator<Item = f32>) -> f32 {
    percentile(values.filter(|v| *v > 0.0).collect(), 0.98).unwrap_or(1.0).max(1e-6)
}

/// A value against its top, 0..=255.
fn against(v: f32, top: f32) -> u8 {
    ((v / top).clamp(0.0, 1.0) * 255.0).round() as u8
}

fn median(v: &mut [f32]) -> f32 {
    let mid = v.len() / 2;
    *v.select_nth_unstable_by(mid, f32::total_cmp).1
}

/// Frames of a series at `rate` over `duration` (at least one).
fn frames_in(duration: f64, rate: usize) -> usize {
    ((duration * rate as f64).ceil() as usize).max(1)
}

/// The frame at `rate` a moment `t` (s) falls in.
fn bucket(t: f64, rate: usize, count: usize) -> usize {
    ((t.max(0.0) * rate as f64) as usize).min(count - 1)
}

/// Hits from a band's flux, frame by frame (Bello et al. 2005, as Böck et al. 2012 pick them): a frame whose flux is
/// the largest within 35 ms either side, above the median of the 120 ms before and 60 ms after it by a tenth of the
/// band's top, and not under a sixth of that top; one per `gap_ms`, the strongest. Its time between frames from the
/// parabola through it and its neighbours. Strength against the band's strong hits (their 90th percentile and up are
/// 255).
fn pick(flux: &[f32], onsets: &[f64], gap_ms: u32, hop: usize, rate: u32) -> Vec<(u32, u8)> {
    let top = top_of(flux.iter().copied());
    let frames_of = |ms: f64| ((ms / 1000.0 * f64::from(rate) / hop as f64).round() as usize).max(1);
    let (near, before, after) = (frames_of(35.0), frames_of(120.0), frames_of(60.0));
    let mut window = Vec::new();
    let mut picked: Vec<(usize, f32)> = Vec::new();
    for (i, &f) in flux.iter().enumerate() {
        if f < top / 6.0 {
            continue;
        }
        let (lo, hi) = (i.saturating_sub(near), (i + near + 1).min(flux.len()));
        if flux[lo..i].iter().any(|&x| x >= f) || flux[i + 1..hi].iter().any(|&x| x > f) {
            continue;
        }
        window.clear();
        window.extend_from_slice(&flux[i.saturating_sub(before)..(i + after + 1).min(flux.len())]);
        if f < median(&mut window) + top / 10.0 {
            continue;
        }
        match picked.last_mut() {
            Some(last) if (onsets[i] - onsets[last.0]) * 1000.0 < f64::from(gap_ms) => {
                if f > last.1 {
                    *last = (i, f);
                }
            }
            _ => picked.push((i, f)),
        }
    }
    let strong = percentile(picked.iter().map(|p| p.1).collect(), 0.9).unwrap_or(1.0).max(1e-6);
    picked
        .into_iter()
        .map(|(i, f)| {
            let (l, r) = (if i > 0 { flux[i - 1] } else { f }, flux.get(i + 1).copied().unwrap_or(f));
            let curve = l - 2.0 * f + r;
            let between = if curve < 0.0 { (0.5 * (l - r) / curve).clamp(-0.5, 0.5) } else { 0.0 };
            let t = onsets[i] + f64::from(between) * hop as f64 / f64::from(rate);
            ((t.max(0.0) * 1000.0).round() as u32, ((f / strong).clamp(0.0, 1.0) * 255.0).round() as u8)
        })
        .collect()
}

/// The tonal centroid's movement (Harte et al. 2006), ten a second: each tenth's chroma, smoothed over the tenths
/// around it, as a point in six dimensions (the circles of fifths, of minor and of major thirds); the change is how far
/// that point moves between the tenth before and the tenth after.
fn harmonic_change(chroma: &[[f32; 12]]) -> Vec<f32> {
    let n = chroma.len();
    // A Gaussian over five tenths either side (σ = 2).
    let weights: Vec<f32> = (-5i32..=5).map(|d| (-(d * d) as f32 / 8.0).exp()).collect();
    let smooth: Vec<[f32; 12]> = (0..n)
        .map(|i| {
            let mut s = [0f32; 12];
            for (j, w) in (-5i32..=5).zip(&weights) {
                let at = i as i64 + i64::from(j);
                if (0..n as i64).contains(&at) {
                    for (v, c) in s.iter_mut().zip(&chroma[at as usize]) {
                        *v += w * c;
                    }
                }
            }
            s
        })
        .collect();
    let centroid = |c: &[f32; 12]| -> [f32; 6] {
        let total: f32 = c.iter().sum();
        let mut out = [0f32; 6];
        if total <= 1e-9 {
            return out;
        }
        for (l, v) in c.iter().enumerate() {
            let l = l as f32;
            let w = v / total;
            let pi = std::f32::consts::PI;
            out[0] += w * (l * 7.0 * pi / 6.0).sin();
            out[1] += w * (l * 7.0 * pi / 6.0).cos();
            out[2] += w * (l * 3.0 * pi / 2.0).sin();
            out[3] += w * (l * 3.0 * pi / 2.0).cos();
            out[4] += 0.5 * w * (l * 2.0 * pi / 3.0).sin();
            out[5] += 0.5 * w * (l * 2.0 * pi / 3.0).cos();
        }
        out
    };
    let points: Vec<[f32; 6]> = smooth.iter().map(centroid).collect();
    (0..n)
        .map(|i| {
            if i == 0 || i + 1 >= n {
                return 0.0;
            }
            points[i + 1].iter().zip(&points[i - 1]).map(|(a, b)| (a - b) * (a - b)).sum::<f32>().sqrt()
        })
        .collect()
}

/// Peaks: the largest within `around` either side, above the mean plus half the deviation and above `floor`, at
/// least `around` apart.
fn peaks(v: &[f32], around: usize, floor: f32) -> Vec<usize> {
    if v.is_empty() {
        return Vec::new();
    }
    let mean = v.iter().sum::<f32>() / v.len() as f32;
    let deviation = (v.iter().map(|x| (x - mean) * (x - mean)).sum::<f32>() / v.len() as f32).sqrt();
    let bar = (mean + 0.5 * deviation).max(floor).max(1e-6);
    let mut out: Vec<usize> = Vec::new();
    for i in 0..v.len() {
        let (lo, hi) = (i.saturating_sub(around), (i + around + 1).min(v.len()));
        if v[i] < bar || v[lo..i].iter().any(|&x| x >= v[i]) || v[i + 1..hi].iter().any(|&x| x > v[i]) {
            continue;
        }
        if out.last().is_none_or(|l| i - l >= around) {
            out.push(i);
        }
    }
    out
}

/// The sections' novelty at each unit (a beat, or half a second), Foote 2000: each unit's chroma and timbre (its MFCC,
/// each coefficient standardised over the track), how alike each unit is to each near it, and a Gaussian-tapered
/// checkerboard along that: high where what came before is like itself and unlike what follows.
fn novelty(units: &[f64], duration: f64, chroma: &[[f32; 12]], mfcc: &[[f32; MFCC]]) -> Vec<f32> {
    let n = units.len();
    if n == 0 {
        return Vec::new();
    }
    let tenth = |t: f64| ((t.max(0.0) * TENTHS as f64) as usize).min(chroma.len().saturating_sub(1));
    // Standardised timbre.
    let count = mfcc.len().max(1) as f32;
    let mut mean = [0f32; MFCC];
    for m in mfcc {
        for (s, v) in mean.iter_mut().zip(m) {
            *s += v / count;
        }
    }
    let mut spread = [0f32; MFCC];
    for m in mfcc {
        for ((s, v), mu) in spread.iter_mut().zip(m).zip(&mean) {
            *s += (v - mu) * (v - mu) / count;
        }
    }
    let spread = spread.map(|s| s.sqrt().max(1e-6));
    let features: Vec<Vec<f32>> = (0..n)
        .map(|i| {
            let (from, to) = (tenth(units[i]), tenth(units.get(i + 1).copied().unwrap_or(duration)).max(tenth(units[i]) + 1).min(chroma.len()));
            let mut c = [0f32; 12];
            let mut t = [0f32; MFCC];
            for j in from..to.max(from + 1).min(chroma.len().max(from + 1)) {
                if let (Some(cj), Some(tj)) = (chroma.get(j), mfcc.get(j)) {
                    for (s, v) in c.iter_mut().zip(cj) {
                        *s += v;
                    }
                    for ((s, v), (mu, sd)) in t.iter_mut().zip(tj).zip(mean.iter().zip(&spread)) {
                        *s += (v - mu) / sd;
                    }
                }
            }
            let cn = c.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-9);
            let tn = t.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-9);
            // Chroma and timbre weigh alike.
            let mut f: Vec<f32> = c.iter().map(|v| v / cn).collect();
            f.extend(t.iter().map(|v| v / tn));
            f
        })
        .collect();
    let alike = |a: usize, b: usize| features[a].iter().zip(&features[b]).map(|(x, y)| x * y).sum::<f32>() / 2.0;
    let k = KERNEL as i64;
    let sigma = KERNEL as f32 / 2.0;
    (0..n)
        .map(|i| {
            // The kernel as large as fits on both sides alike: a lopsided one would read a section's own likeness as
            // a change (at the very start, none at all).
            let l = k.min(i as i64).min(n as i64 - i as i64);
            let mut sum = 0f32;
            for a in -l..l {
                for b in -l..l {
                    let (x, y) = (i as i64 + a, i as i64 + b);
                    if x < 0 || y < 0 || x >= n as i64 || y >= n as i64 {
                        continue;
                    }
                    let sign = if (a < 0) == (b < 0) { 1.0 } else { -1.0 };
                    let (fa, fb) = (a as f32 + 0.5, b as f32 + 0.5);
                    let taper = (-(fa * fa + fb * fb) / (2.0 * sigma * sigma)).exp();
                    sum += sign * taper * alike(x as usize, y as usize);
                }
            }
            sum.max(0.0)
        })
        .collect()
}

/// Each beat's place in its bar from the downbeats (each at a beat): 0 at a downbeat, counting up to the next. Before
/// the first, counted back from it by the bar most often heard (four when there is no whole bar).
fn places(beats: &[f64], downbeats: &[f64]) -> Vec<u8> {
    let down: Vec<bool> = beats.iter().map(|b| downbeats.iter().any(|d| (d - b).abs() < 0.015)).collect();
    let at: Vec<usize> = down.iter().enumerate().filter(|(_, d)| **d).map(|(i, _)| i).collect();
    let mut lengths: BTreeMap<usize, usize> = BTreeMap::new();
    for w in at.windows(2) {
        *lengths.entry(w[1] - w[0]).or_default() += 1;
    }
    let meter = lengths.iter().max_by_key(|(len, n)| (**n, std::cmp::Reverse(**len))).map_or(4, |(len, _)| *len).max(1);
    let first = at.first().copied().unwrap_or(0);
    let mut out = Vec::with_capacity(beats.len());
    let mut place = 0usize;
    for (i, d) in down.iter().enumerate() {
        if i < first {
            out.push(((meter - (first - i) % meter) % meter).min(255) as u8);
            continue;
        }
        place = if *d { 0 } else { place + 1 };
        out.push(place.min(255) as u8);
    }
    out
}

/// Each beat's place in its bar: of three and four beats a bar, and each first beat among them, the one whose first
/// beats carry the most of the low bands' starts against all beats. Returns the meter, the phase (the first downbeat)
/// and how sure (0..1: how far the best phase stands above the next of its meter).
fn bar(at_beat: &[f32]) -> (usize, usize, f32) {
    let all = at_beat.iter().sum::<f32>() / at_beat.len().max(1) as f32;
    if all <= 1e-9 {
        return (4, 0, 0.0);
    }
    let score = |meter: usize, phase: usize| {
        let picked: Vec<f32> = at_beat.iter().enumerate().filter(|(i, _)| i % meter == phase).map(|(_, v)| *v).collect();
        picked.iter().sum::<f32>() / picked.len().max(1) as f32 / all
    };
    let best = |meter: usize| -> (usize, f32, f32) {
        let mut scores: Vec<(usize, f32)> = (0..meter).map(|p| (p, score(meter, p))).collect();
        scores.sort_by(|a, b| b.1.total_cmp(&a.1));
        (scores[0].0, scores[0].1, scores[1].1)
    };
    let (p4, s4, n4) = best(4);
    let (p3, s3, n3) = best(3);
    // Four unless three stands out clearly more.
    if s3 > s4 * 1.1 { (3, p3, ((s3 - n3) / s3).max(0.0)) } else { (4, p4, ((s4 - n4) / s4).max(0.0)) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    /// `secs` of sound at `rate`, each sample's middle and side from `f(t)`, listened to with these beats.
    fn heard(rate: u32, secs: f64, beats: &[f64], mut f: impl FnMut(f64) -> (f64, f64)) -> BTreeMap<u8, Section> {
        let mut ear = SignalEar::new(rate);
        for i in 0..(secs * f64::from(rate)) as usize {
            let (m, s) = f(i as f64 / f64::from(rate));
            ear.hear(m as f32, s as f32);
        }
        ear.finish(&[], beats, None).read().expect("reads back")
    }

    fn frames(s: &BTreeMap<u8, Section>, id: u8) -> Vec<Vec<u8>> {
        s[&id].frames().into_iter().map(<[u8]>::to_vec).collect()
    }

    fn hits(s: &BTreeMap<u8, Section>, id: u8) -> Vec<Vec<(u32, u8)>> {
        match &s[&id].data {
            Data::Hits(h) => h.clone(),
            _ => panic!("not hits"),
        }
    }

    /// A small random number generator, the same every run.
    fn noise(seed: u64) -> impl FnMut() -> f64 {
        let mut x = seed.max(1);
        move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
        }
    }

    #[test]
    fn a_tone_lands_in_its_own_band_at_any_rate() {
        // A second and a half each, near each band's middle.
        let tones = [35.0, 95.0, 245.0, 900.0, 3200.0, 10_000.0];
        for rate in [44_100, 96_000, 192_000] {
            let s = heard(rate, 9.0, &[], |t| (0.3 * (2.0 * PI * tones[((t / 1.5) as usize).min(5)] * t).sin(), 0.0));
            let level = frames(&s, id::LEVEL);
            assert_eq!(level.len(), 180, "20 a second");
            for (section, &tone) in tones.iter().enumerate() {
                for frame in &level[section * 30 + 8..section * 30 + 22] {
                    assert!(frame[section] >= 250, "{rate} Hz, {tone} Hz: its own band at {}", frame[section]);
                    for (b, &v) in frame.iter().enumerate() {
                        // A neighbour hears a tone in the middle of its band about 15 dB down (24 dB an octave at
                        // each edge, the usual crossover slope); the bands beyond, next to nothing.
                        let most = match b.abs_diff(section) {
                            0 => continue,
                            1 => 140,
                            _ => 40,
                        };
                        assert!(v <= most, "{rate} Hz, {tone} Hz: band {b} at {v} (leaks)");
                    }
                }
            }
            assert_eq!(s[&id::BAND_TABLE].data, Data::Table(EDGES.to_vec()));
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

    /// Of `got` (ms), how far each is from the nearest of `want` (s), and how many of `want` are within 15 ms of one.
    fn matched(got: &[(u32, u8)], want: &[f64]) -> (Vec<f64>, usize) {
        let got: Vec<f64> = got.iter().map(|(ms, _)| f64::from(*ms) / 1000.0).collect();
        // Signed (late is positive), the nearest of `want` each.
        let off = got.iter().map(|g| want.iter().map(|w| g - w).fold(f64::MAX, |a, b| if b.abs() < a.abs() { b } else { a })).collect();
        (off, want.iter().filter(|w| got.iter().any(|g| (g - *w).abs() <= 0.015)).count())
    }

    #[test]
    fn a_kick_a_snare_and_hats_are_each_found_in_their_own_band() {
        // 120 BPM over 12 s: a kick (55 Hz, rising over 8 ms as a kick drum does) on every beat, a snare (900 and
        // 1300 Hz) on two and four, hats (7.5, 9 and 11 kHz) on the eighths between.
        let kicks: Vec<f64> = (0..24).map(|k| 0.5 + 0.5 * f64::from(k)).collect();
        let snares: Vec<f64> = (0..12).map(|k| 1.0 + 1.0 * f64::from(k)).collect();
        let hats: Vec<f64> = (0..24).map(|k| 0.75 + 0.5 * f64::from(k)).collect();
        for rate in [44_100, 96_000, 192_000] {
            let s = heard(rate, 12.5, &[], |t| {
                let near = |at: &[f64]| at.iter().copied().filter(|s| *s <= t).next_back();
                let m = 0.5 * near(&kicks).map_or(0.0, |s| burst(t, s, 0.008, 0.08, &[55.0]))
                    + 0.3 * near(&snares).map_or(0.0, |s| burst(t, s, 0.002, 0.05, &[900.0, 1300.0]))
                    + 0.2 * near(&hats).map_or(0.0, |s| burst(t, s, 0.001, 0.01, &[7500.0, 9000.0, 11_000.0]));
                (m, 0.0)
            });
            let found = hits(&s, id::HITS);
            for (band, want) in [(0, &kicks), (3, &snares), (5, &hats)] {
                let (off, caught) = matched(&found[band], want);
                assert!(off.iter().all(|o: &f64| o.abs() <= 0.015), "{rate} Hz, band {band}: every hit within 15 ms of its own: {off:?}");
                assert!(caught * 10 >= want.len() * 9, "{rate} Hz, band {band}: {caught} of {}", want.len());
            }
        }
    }

    #[test]
    fn a_held_tone_is_harmonic_and_clicks_are_percussive() {
        // A 440 Hz tone all through, and a click every half second.
        let clicks: Vec<f64> = (0..15).map(|k| 0.5 + 0.5 * f64::from(k)).collect();
        let rate = 44_100;
        let s = heard(rate, 8.0, &[], |t| {
            let click = clicks.iter().any(|c| t >= *c && t < c + 2.0 / f64::from(rate));
            (0.2 * (2.0 * PI * 440.0 * t).sin() + if click { 0.9 } else { 0.0 }, 0.0)
        });
        let (percussive, harmonic) = (frames(&s, id::PERCUSSIVE), frames(&s, id::HARMONIC));
        // The middle band (the tone's): between the clicks the harmonic part well over the percussive.
        for f in 10..70 {
            let between = clicks.iter().all(|c| (f as f64 / 10.0 - c).abs() > 0.15);
            if between {
                assert!(harmonic[f][3] >= percussive[f][3].saturating_add(60), "tenth {f}: harmonic {} percussive {}", harmonic[f][3], percussive[f][3]);
            }
        }
        // Air (the clicks' alone): percussive over harmonic where the clicks are, and the percussive part's hits on them.
        for c in &clicks {
            let f = (c * 10.0) as usize;
            assert!(percussive[f][5] > harmonic[f][5], "click at {c}: percussive {} harmonic {}", percussive[f][5], harmonic[f][5]);
        }
        let (off, caught) = matched(&hits(&s, id::PERCUSSIVE_HITS)[5], &clicks);
        assert!(off.iter().all(|o: &f64| o.abs() <= 0.015) && caught >= 14, "{off:?}, {caught}");
    }

    #[test]
    fn chroma_names_a_chords_notes() {
        // C major: C4, E4, G4.
        let s = heard(44_100, 3.0, &[], |t| ([261.63, 329.63, 392.0].iter().map(|f| 0.2 * (2.0 * PI * f * t).sin()).sum(), 0.0));
        let chroma = frames(&s, id::CHROMA);
        for frame in &chroma[2..13] {
            let mut order: Vec<usize> = (0..12).collect();
            order.sort_by_key(|i| std::cmp::Reverse(frame[*i]));
            let mut top = order[..3].to_vec();
            top.sort();
            assert_eq!(top, vec![0, 4, 7], "{frame:?}");
        }
    }

    #[test]
    fn two_unlike_halves_have_one_boundary_where_they_meet() {
        // 30 s of a soft C major chord in sines, then 30 s of an F sharp major chord in square waves with noise.
        let mut rnd = noise(7);
        let square = |f: f64, t: f64| if (f * t).fract() < 0.5 { 1.0 } else { -1.0 };
        let s = heard(22_050, 60.0, &[], move |t| {
            let m = if t < 30.0 {
                [261.63, 329.63, 392.0].iter().map(|f| 0.15 * (2.0 * PI * f * t).sin()).sum::<f64>()
            } else {
                [185.0, 233.08, 277.18].iter().map(|f| 0.08 * square(*f, t)).sum::<f64>() + 0.05 * rnd()
            };
            (m, 0.0)
        });
        assert_eq!(s[&id::NOVELTY].rate, (UNBEATEN_RATE * 10) as u16, "no beats: two a second");
        let Data::Marks(marks) = &s[&id::BOUNDARIES].data else { panic!() };
        let near: Vec<f64> = marks.iter().map(|(ms, _)| f64::from(*ms) / 1000.0).filter(|t| (t - 30.0).abs() <= 1.5).collect();
        assert!(!near.is_empty(), "a boundary at the join: {marks:?}");
        assert!(marks.iter().all(|(ms, _)| (f64::from(*ms) / 1000.0 - 30.0).abs() <= 6.0), "none elsewhere: {marks:?}");
        // The harmonic change peaks there too.
        let Data::Marks(changes) = &s[&id::HARMONIC_CHANGES].data else { panic!() };
        assert!(changes.iter().any(|(ms, _)| (f64::from(*ms) / 1000.0 - 30.0).abs() <= 1.0), "{changes:?}");
    }

    #[test]
    fn width_is_none_in_mono_half_for_unrelated_channels_and_all_for_opposites() {
        let rate = 22_050;
        let mut a = noise(3);
        let mono = heard(rate, 3.0, &[], move |_| (0.3 * a(), 0.0));
        // Two unrelated channels: middle and side alike strong.
        let (mut b, mut c) = (noise(11), noise(5));
        let wide = heard(rate, 3.0, &[], move |_| {
            let (l, r) = (0.3 * b(), 0.3 * c());
            ((l + r) / 2.0, (l - r) / 2.0)
        });
        // One channel the other's opposite: no middle at all.
        let mut d = noise(13);
        let opposite = heard(rate, 3.0, &[], move |_| (0.0, 0.3 * d()));
        for (s, want) in [(&mono, 0.0), (&wide, 0.5), (&opposite, 1.0)] {
            for frame in &frames(s, id::WIDTH)[2..13] {
                for &v in &frame[2..] {
                    assert!((f64::from(v) / 255.0 - want).abs() <= 0.1, "want {want}: {frame:?}");
                }
            }
        }
    }

    #[test]
    fn clicks_have_more_crest_than_a_held_tone() {
        let rate = 22_050;
        let tone = heard(rate, 3.0, &[], |t| (0.3 * (2.0 * PI * 900.0 * t).sin(), 0.0));
        let clicks = heard(rate, 3.0, &[], |t| (if (t * 10.0).fract() < 0.005 { 0.9 } else { 0.0 }, 0.0));
        for (f, g) in frames(&tone, id::CREST)[2..13].iter().zip(&frames(&clicks, id::CREST)[2..13]) {
            // A sine's crest is 3 dB (about 25 of 255); clicks' far more.
            assert!(f[3] <= 40, "{f:?}");
            assert!(g[3] >= 150, "{g:?}");
        }
    }

    #[test]
    fn the_bar_follows_the_accented_beat() {
        // Every fourth beat from the second carries the most.
        let at: Vec<f32> = (0..32).map(|i| if i % 4 == 1 { 3.0 } else { 1.0 }).collect();
        let (meter, phase, sure) = bar(&at);
        assert_eq!((meter, phase), (4, 1));
        assert!(sure > 0.5, "{sure}");
        // Three a bar.
        let at: Vec<f32> = (0..30).map(|i| if i % 3 == 0 { 3.0 } else { 1.0 }).collect();
        assert_eq!(bar(&at).0, 3);
        // Nothing stands out: unsure.
        assert!(bar(&[1.0; 32]).2 < 0.05);
    }

    #[test]
    fn beat_this_downbeats_give_each_beat_its_place() {
        // Bars of four, the first downbeat at the third beat; a bar of three in the middle.
        let beats: Vec<f64> = (0..14).map(|k| 0.5 * f64::from(k)).collect();
        let downbeats = [1.0, 3.0, 4.5, 6.5];
        assert_eq!(places(&beats, &downbeats), vec![2, 3, 0, 1, 2, 3, 0, 1, 2, 0, 1, 2, 3, 0]);
        // Sure, in the section's scale byte.
        let mut ear = SignalEar::new(22_050);
        for _ in 0..22_050 * 7 {
            ear.hear(0.0, 0.0);
        }
        let s = ear.finish(&[], &beats, Some(&downbeats)).read().unwrap();
        assert_eq!(s[&id::BAR].scale, 255);
        assert_eq!(frames(&s, id::BAR).concat(), places(&beats, &downbeats));
    }

    #[test]
    fn a_steady_rise_is_a_swell_and_a_single_hit_is_not() {
        // Ten a second: 3 s steady at -30 dB, 4 s rising to -15, 3 s steady; then a 0.2 s jump to -10 and back.
        let mut level: Vec<f32> = vec![-30.0; 30];
        level.extend((0..40).map(|i| -30.0 + 15.0 * i as f32 / 40.0));
        level.extend(vec![-15.0; 30]);
        level.extend(vec![-10.0; 2]);
        level.extend(vec![-15.0; 30]);
        let s = swell(&level);
        assert!(s[..28].iter().all(|v| *v == 0.0), "steady: none");
        assert!(s[66..71].iter().all(|v| *v > 0.7), "the end of the rise: {:?}", &s[60..72]);
        assert!(s[102..].iter().all(|v| *v == 0.0), "a single jump is no swell: {:?}", &s[98..110]);
    }

    #[test]
    fn silence_has_quiet_signals_and_no_hits_and_beats_give_a_bar() {
        let beats: Vec<f64> = (0..20).map(|k| 0.5 * f64::from(k)).collect();
        let s = heard(22_050, 10.0, &beats, |_| (0.0, 0.0));
        for id in [id::LEVEL, id::FLUX, id::PERCUSSIVE, id::HARMONIC, id::CREST, id::WIDTH, id::CENTROID, id::CHROMA, id::SWELL, id::BUILD_UP] {
            assert!(frames(&s, id).iter().all(|f| f.iter().all(|v| *v == 0)), "section {id}");
        }
        assert!(hits(&s, id::HITS).iter().all(Vec::is_empty) && hits(&s, id::PERCUSSIVE_HITS).iter().all(Vec::is_empty));
        // With beats: novelty a beat (rate 0), and each beat's place in its bar.
        assert_eq!((s[&id::NOVELTY].rate, frames(&s, id::NOVELTY).len()), (0, 20));
        assert_eq!(frames(&s, id::BAR).len(), 20);
        assert_eq!(Signals::default().read(), None, "nothing (a partial decode) reads as nothing");
        let sections: Vec<u8> = s.keys().copied().collect();
        assert_eq!(sections, (0..=19).collect::<Vec<u8>>());
    }
}
