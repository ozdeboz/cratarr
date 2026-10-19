//! Beat this! (decision 0049, amended 2026-10-19; Foscarin, Schlüter, Widmer, ISMIR 2024; CPJKU's code and weights,
//! MIT): the beats and downbeats the visuals' Beat, Downbeat and Tempo follow, in place of the analysis' own grid.
//!
//! The library's are heard on the owner's PC by the official package (`tools/beat_this_pass.py`; the owner: "the
//! library on the PC, new music on the NAS"): it reads the list this server writes, `<data>/beats/wanted.jsonl` (the
//! files without them, as the server sees them), and sends back JSON-lines files over SSH into `<data>/beats/in/`,
//! each taken in and then removed. New music is heard here, by the same small model (`small0`, as ONNX, through tract)
//! on the same spectrogram, beside the analysis' pass (`analysis_pass`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use tract_onnx::prelude::*;

use crate::analysis::Heard;
use crate::catalogue::{BeatThisRow, Catalogue};

/// The list of files without Beat this!'s beats, in Cratarr's data folder: what the PC's pass hears.
pub const WANTED: &str = "beats/wanted.jsonl";
/// Where the PC's pass puts what it heard, a file at a time (`*.jsonl`; anything else is left alone).
pub const INBOX: &str = "beats/in";
/// The catalogue's note of the list as last written (its count and hash).
const WANTED_STAMP: &str = "beat_this_wanted";
/// The catalogue's note of when Cratarr first heard new music itself: what came or changed before is the PC's.
const SINCE: &str = "beat_this_since";

/// One file, as the PC's pass writes it: the file as the server listed it, the beats and downbeats in milliseconds.
#[derive(Deserialize)]
struct Line {
    /// The library folder as the server sees it (`/media/Music`).
    folder: String,
    rel: String,
    size: u64,
    mtime: i64,
    beats: Vec<u32>,
    #[serde(default)]
    downbeats: Vec<u32>,
}

/// What the waiting files brought in.
#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
pub struct Counts {
    pub files: u64,
    pub stored: u64,
    /// The file is there, but not as it was heard (another size or mtime).
    pub stale: u64,
    /// No such file (not yet seen, or gone).
    pub unknown: u64,
    /// Not a line of this kind.
    pub bad: u64,
}

/// Takes in every file waiting in the inbox, then removes it; `None` when none was waiting. Blocking.
pub fn import_waiting(cat: &Catalogue, data: &Path) -> Result<Option<Counts>> {
    let Ok(entries) = std::fs::read_dir(data.join(INBOX)) else { return Ok(None) };
    let mut files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "jsonl")).collect();
    if files.is_empty() {
        return Ok(None);
    }
    files.sort();
    let mut counts = Counts::default();
    for file in files {
        let text = std::fs::read_to_string(&file)?;
        let rows = parse(cat, &text, &mut counts);
        cat.store_beat_this(&rows)?;
        counts.stored += rows.len() as u64;
        counts.files += 1;
        std::fs::remove_file(&file)?;
    }
    Ok(Some(counts))
}

fn parse(cat: &Catalogue, text: &str, counts: &mut Counts) -> Vec<BeatThisRow> {
    let mut rows = Vec::new();
    for raw in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let Ok(line) = serde_json::from_str::<Line>(raw) else {
            counts.bad += 1;
            continue;
        };
        let Some(folder) = crate::app::analysis_import::folder_of(cat, &line.folder) else {
            counts.unknown += 1;
            continue;
        };
        match cat.file_stamp(folder, &line.rel) {
            None => counts.unknown += 1,
            Some((size, mtime)) if size == line.size && mtime == line.mtime => {
                rows.push(BeatThisRow { folder, rel: line.rel, size, mtime, beats: line.beats, downbeats: line.downbeats })
            }
            Some(_) => counts.stale += 1,
        }
    }
    rows
}

/// Writes the list of files without Beat this!'s beats when it changed: one JSON line each, `{"folder", "rel",
/// "size", "mtime"}`, newest first; in one move. How many it lists, or `None` when it had not changed. Blocking.
pub fn write_wanted(cat: &Catalogue, data: &Path) -> Result<Option<usize>> {
    let rows = cat.without_beat_this(None, usize::MAX)?;
    let folders = cat.folders();
    let mut text = String::new();
    for (folder, rel, _, size, mtime) in &rows {
        let Some(root) = usize::try_from(*folder).ok().and_then(|i| folders.get(i)) else { continue };
        let line = serde_json::json!({ "folder": root.to_string_lossy(), "rel": rel, "size": size, "mtime": mtime });
        text.push_str(&line.to_string());
        text.push('\n');
    }
    let path = data.join(WANTED);
    // FNV-1a: only to see whether the list changed.
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    let stamp = format!("{}:{hash:016x}", rows.len());
    if path.exists() && cat.meta(WANTED_STAMP).as_deref() == Some(stamp.as_str()) {
        return Ok(None);
    }
    let folder = path.parent().ok_or_else(|| anyhow!("no folder for {}", path.display()))?;
    std::fs::create_dir_all(folder)?;
    std::fs::create_dir_all(data.join(INBOX))?;
    let part = path.with_extension("jsonl.part");
    std::fs::write(&part, text)?;
    std::fs::rename(&part, &path)?;
    cat.set_meta(WANTED_STAMP, &stamp);
    Ok(Some(rows.len()))
}

/// New music to hear here: files first seen or changed since Cratarr first looked for it (noted then, so what was
/// there before is the PC's), without Beat this!'s beats; newest first, at most `limit`. Blocking.
pub fn new_music(cat: &Catalogue, limit: usize) -> Result<Vec<(i64, String, PathBuf, u64, i64)>> {
    let since = match cat.meta(SINCE).and_then(|s| s.parse::<i64>().ok()) {
        Some(s) => s,
        None => {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64;
            cat.set_meta(SINCE, &now.to_string());
            now
        }
    };
    cat.without_beat_this(Some(since), limit)
}

/// A file heard, as kept. Blocking.
pub fn hear_and_keep(cat: &Catalogue, folder: i64, rel: &str, path: &Path, size: u64, mtime: i64) -> Result<usize> {
    let heard = hear(path)?;
    let millis = |v: &[f64]| v.iter().map(|t| (t.max(0.0) * 1000.0).round() as u32).collect::<Vec<u32>>();
    let row = BeatThisRow { folder, rel: rel.to_string(), size, mtime, beats: millis(&heard.beats), downbeats: millis(&heard.downbeats) };
    cat.store_beat_this(std::slice::from_ref(&row))?;
    Ok(row.beats.len())
}

// ---- The model ----

/// Beat this!'s small model (`small0`), exported from CPJKU's checkpoint to ONNX with its time axis free
/// (`server/models/README.md`).
static MODEL: &[u8] = include_bytes!("../../models/beat-this-small0.onnx");

/// The spectrogram the model was trained on (`beat_this.preprocessing.LogMelSpect`): 22,050 Hz mono, a 1024-sample
/// Hann window every 441 samples (50 frames a second), magnitudes over √1024, 128 mel bands (Slaney's scale, unnormed)
/// from 30 Hz to 11 kHz, then ln(1 + 1000·x).
const RATE: u32 = 22_050;
const N_FFT: usize = 1024;
const HOP: usize = 441;
const MELS: usize = 128;
const F_MIN: f32 = 30.0;
const F_MAX: f32 = 11_000.0;
const FPS: f64 = 50.0;
/// The model hears 30 s at a time; each piece's first and last six frames are discarded, so pieces overlap by them.
const CHUNK: usize = 1500;
const BORDER: usize = 6;

type Plan = TypedRunnableModel<TypedModel>;

/// The model for pieces of `frames` frames.
fn plan(frames: usize) -> Result<Plan> {
    tract_onnx::onnx()
        .model_for_read(&mut std::io::Cursor::new(MODEL))
        .and_then(|m| m.with_input_fact(0, f32::fact([1, frames, MELS]).into()))
        .and_then(|m| m.into_optimized())
        .and_then(|m| m.into_runnable())
        .map_err(|e| anyhow!("the beat model: {e}"))
}

/// The model for whole pieces, made once.
fn whole() -> Result<Arc<Plan>> {
    static WHOLE: OnceLock<std::result::Result<Arc<Plan>, String>> = OnceLock::new();
    WHOLE.get_or_init(|| plan(CHUNK).map(Arc::new).map_err(|e| format!("{e:#}"))).clone().map_err(|e| anyhow!(e))
}

/// A file's beats and downbeats as Beat this! hears them (its minimal post-processing, without a DBN). Blocking.
pub fn hear(path: &Path) -> Result<Heard> {
    hear_timed(path).map(|(h, _, _)| h)
}

/// `hear`, with the seconds the spectrogram and the model took.
pub fn hear_timed(path: &Path) -> Result<(Heard, f64, f64)> {
    let started = std::time::Instant::now();
    let spect = spectrogram(path)?;
    let spect_secs = started.elapsed().as_secs_f64();
    let (beat, down) = logits(&spect)?;
    Ok((beats_of(&beat, &down), spect_secs, started.elapsed().as_secs_f64() - spect_secs))
}

/// The file's spectrogram, frame after frame (`MELS` values each).
fn spectrogram(path: &Path) -> Result<Vec<f32>> {
    let mut mono = crate::analysis::Mono::open(path)?;
    let rate = mono.rate();
    let mut stft = Stft::new();
    crate::convert::stream(|bufs: &mut [Vec<f32>]| mono.read(&mut bufs[0]), 1, rate, RATE, &mut |out: &[Vec<f32>], from: usize, n: usize| {
        stft.push(&out[0][from..from + n]);
        Ok(())
    })?;
    stft.finish()
}

/// The short-time Fourier transform to the log mel spectrogram, as torchaudio's with its window centred: the sound is
/// mirrored by half a window at either end.
struct Stft {
    fft: Arc<dyn realfft::RealToComplex<f32>>,
    window: Vec<f32>,
    /// Each band's bins and weights.
    bank: Vec<Vec<(usize, f32)>>,
    /// The first half window and one of the sound, until the start can be mirrored.
    head: Vec<f32>,
    started: bool,
    /// The mirrored sound from `base` on (in samples of it).
    padded: Vec<f32>,
    base: usize,
    /// Where the next frame starts.
    next: usize,
    frames: Vec<f32>,
    input: Vec<f32>,
    output: Vec<realfft::num_complex::Complex<f32>>,
}

impl Stft {
    fn new() -> Self {
        let fft = realfft::RealFftPlanner::<f32>::new().plan_fft_forward(N_FFT);
        let window = (0..N_FFT).map(|n| (0.5 - 0.5 * (2.0 * std::f64::consts::PI * n as f64 / N_FFT as f64).cos()) as f32).collect();
        let (input, output) = (fft.make_input_vec(), fft.make_output_vec());
        Stft { fft, window, bank: mel_bank(), head: Vec::new(), started: false, padded: Vec::new(), base: 0, next: 0, frames: Vec::new(), input, output }
    }

    fn push(&mut self, sound: &[f32]) {
        if !self.started {
            self.head.extend_from_slice(sound);
            if self.head.len() <= N_FFT / 2 {
                return;
            }
            // x[512], x[511], …, x[1], then the sound.
            self.padded.extend((1..=N_FFT / 2).rev().map(|i| self.head[i]));
            let head = std::mem::take(&mut self.head);
            self.padded.extend_from_slice(&head);
            self.started = true;
        } else {
            self.padded.extend_from_slice(sound);
        }
        self.frame_all();
    }

    fn finish(mut self) -> Result<Vec<f32>> {
        if !self.started {
            return Err(anyhow!("too short to hear beats in"));
        }
        // Then x[len - 2], x[len - 3], …, x[len - 513].
        let len = self.padded.len();
        let tail: Vec<f32> = (2..=N_FFT / 2 + 1).map(|k| self.padded[len - k]).collect();
        self.padded.extend_from_slice(&tail);
        self.frame_all();
        Ok(self.frames)
    }

    fn frame_all(&mut self) {
        while self.next + N_FFT <= self.base + self.padded.len() {
            let at = self.next - self.base;
            for (i, (x, w)) in self.padded[at..at + N_FFT].iter().zip(&self.window).enumerate() {
                self.input[i] = x * w;
            }
            if self.fft.process(&mut self.input, &mut self.output).is_err() {
                self.output.iter_mut().for_each(|c| *c = Default::default());
            }
            let scale = 1.0 / (N_FFT as f32).sqrt();
            for band in &self.bank {
                let mel: f32 = band.iter().map(|(bin, w)| self.output[*bin].norm() * scale * w).sum();
                self.frames.push((1000.0 * mel).ln_1p());
            }
            self.next += HOP;
        }
        // Keep what the next frame needs, and the last half window and one for mirroring the end.
        let keep_from = (self.next - self.base).min(self.padded.len().saturating_sub(N_FFT / 2 + 1));
        self.padded.drain(..keep_from);
        self.base += keep_from;
    }
}

/// torchaudio's `melscale_fbanks` (Slaney's scale, no norm): triangles between mel-spaced corners, over the bins'
/// frequencies from 0 to 11,025 Hz.
fn mel_bank() -> Vec<Vec<(usize, f32)>> {
    const STEP: f32 = 200.0 / 3.0;
    let log_step = 6.4f32.ln() / 27.0;
    let to_mel = |f: f32| if f >= 1000.0 { 15.0 + (f / 1000.0).ln() / log_step } else { f / STEP };
    let to_hz = |m: f32| if m >= 15.0 { 1000.0 * (log_step * (m - 15.0)).exp() } else { STEP * m };
    let bins = N_FFT / 2 + 1;
    let freqs: Vec<f32> = (0..bins).map(|k| (RATE / 2) as f32 * k as f32 / (bins - 1) as f32).collect();
    let (lo, hi) = (to_mel(F_MIN), to_mel(F_MAX));
    let corners: Vec<f32> = (0..MELS + 2).map(|i| to_hz(lo + (hi - lo) * i as f32 / (MELS + 1) as f32)).collect();
    (0..MELS)
        .map(|m| {
            freqs
                .iter()
                .enumerate()
                .filter_map(|(k, f)| {
                    let down = (f - corners[m]) / (corners[m + 1] - corners[m]);
                    let up = (corners[m + 2] - f) / (corners[m + 2] - corners[m + 1]);
                    let w = down.min(up).max(0.0);
                    (w > 0.0).then_some((k, w))
                })
                .collect()
        })
        .collect()
}

/// The model's beat and downbeat logits for every frame (`beat_this.inference.split_predict_aggregate`): 30 s pieces,
/// the last one moved back to end with the sound; where they overlap, the earlier piece's.
fn logits(spect: &[f32]) -> Result<(Vec<f32>, Vec<f32>)> {
    let n = spect.len() / MELS;
    let step = CHUNK - 2 * BORDER;
    let mut starts: Vec<isize> = (0..).map(|k| k as isize * step as isize - BORDER as isize).take_while(|s| *s < n as isize - BORDER as isize).collect();
    if n > step {
        if let Some(last) = starts.last_mut() {
            *last = n as isize - (CHUNK - BORDER) as isize;
        }
    }
    let (mut beat, mut down) = (vec![-1000f32; n], vec![-1000f32; n]);
    let full = whole()?;
    for &start in starts.iter().rev() {
        let (from, to) = (start.max(0) as usize, ((start + CHUNK as isize) as usize).min(n));
        let left = (-start).max(0) as usize;
        let right = (start + CHUNK as isize - n as isize).clamp(0, BORDER as isize) as usize;
        let len = left + (to - from) + right;
        let mut input = vec![0f32; len * MELS];
        input[left * MELS..(left + to - from) * MELS].copy_from_slice(&spect[from * MELS..to * MELS]);
        let tensor: Tensor = tract_ndarray::Array3::from_shape_vec((1, len, MELS), input)?.into();
        let out = if len == CHUNK {
            full.run(tvec!(tensor.into()))
        } else {
            plan(len)?.run(tvec!(tensor.into()))
        }
        .map_err(|e| anyhow!("the beat model: {e}"))?;
        let b = out[0].to_array_view::<f32>().map_err(|e| anyhow!("{e}"))?;
        let d = out[1].to_array_view::<f32>().map_err(|e| anyhow!("{e}"))?;
        let (b, d): (Vec<f32>, Vec<f32>) = (b.iter().copied().collect(), d.iter().copied().collect());
        for i in BORDER..len.saturating_sub(BORDER) {
            let at = start + i as isize;
            if at >= 0 && (at as usize) < n {
                beat[at as usize] = b[i];
                down[at as usize] = d[i];
            }
        }
    }
    Ok((beat, down))
}

/// The minimal post-processing (`beat_this.model.postprocessor`): each logit over 0 that is the largest within three
/// frames either way; next frames' peaks merged to their mean; downbeats moved to the nearest beat (where there are
/// beats; as they are where there are none).
fn beats_of(beat: &[f32], down: &[f32]) -> Heard {
    let peaks = |v: &[f32]| -> Vec<f64> {
        let at: Vec<usize> =
            (0..v.len()).filter(|&i| v[i] > 0.0 && v[i.saturating_sub(3)..(i + 4).min(v.len())].iter().all(|x| *x <= v[i])).collect();
        merge(&at).into_iter().map(|f| f / FPS).collect()
    };
    let beats = peaks(beat);
    let mut downbeats: Vec<f64> = peaks(down)
        .into_iter()
        .map(|d| beats.iter().copied().min_by(|a, b| (a - d).abs().total_cmp(&(b - d).abs())).unwrap_or(d))
        .collect();
    downbeats.dedup();
    Heard { beats, downbeats }
}

/// `deduplicate_peaks(width=1)`: peaks a frame apart from the running mean become that mean.
fn merge(at: &[usize]) -> Vec<f64> {
    let mut out = Vec::new();
    let Some((&first, rest)) = at.split_first() else { return out };
    let (mut p, mut c) = (first as f64, 1.0);
    for &q in rest {
        let q = q as f64;
        if q - p <= 1.0 {
            c += 1.0;
            p += (q - p) / c;
        } else {
            out.push(p);
            (p, c) = (q, 1.0);
        }
    }
    out.push(p);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaks_are_the_largest_within_three_frames_merged_and_downbeats_sit_on_beats() {
        let mut beat = vec![-5f32; 200];
        let mut down = vec![-5f32; 200];
        // Beats at 10, 35 and 36 (one, merged to 35.5), 60; a weaker rise two frames after 60 is no peak.
        for (i, v) in [(10, 2.0), (35, 3.0), (36, 3.0), (60, 4.0), (62, 1.0), (100, -0.5)] {
            beat[i] = v;
        }
        // A downbeat a frame late of 10, and one between beats nearer 60.
        down[11] = 1.0;
        down[55] = 1.0;
        let h = beats_of(&beat, &down);
        assert_eq!(h.beats, vec![0.2, 35.5 / 50.0, 1.2]);
        assert_eq!(h.downbeats, vec![0.2, 1.2]);
        // No beats: the downbeats as heard.
        assert_eq!(beats_of(&[-5.0; 200], &down).downbeats, vec![11.0 / 50.0, 55.0 / 50.0]);
        // Against the running mean, as the reference does: 5 is 1.5 from the mean of 3 and 4.
        assert_eq!(merge(&[3, 4, 5, 9]), vec![3.5, 5.0, 9.0]);
    }

    #[test]
    fn the_mel_bank_spans_30_hz_to_11_khz_in_overlapping_triangles() {
        let bank = mel_bank();
        assert_eq!(bank.len(), MELS);
        let hz = |k: usize| (RATE / 2) as f32 * k as f32 / (N_FFT / 2) as f32;
        assert!(hz(bank[0][0].0) >= 0.0 && hz(bank[0][0].0) < 60.0, "{:?}", bank[0]);
        assert!(hz(bank[MELS - 1].last().unwrap().0) <= F_MAX);
        assert!(bank.iter().flatten().all(|(_, w)| *w > 0.0 && *w <= 1.0));
    }

    #[test]
    fn the_spectrogram_has_a_frame_every_441_samples_and_the_model_hears_a_click_track() {
        // 20 s of clicks at 120 a minute, a louder one every fourth, at 22,050 Hz.
        let n = RATE as usize * 20;
        let sound: Vec<f32> = (0..n)
            .map(|i| {
                let beat = i % (RATE as usize / 2);
                let gain = if (i / (RATE as usize / 2)) % 4 == 0 { 0.9 } else { 0.4 };
                if beat < 200 { gain * (1.0 - beat as f32 / 200.0) * ((i as f32 * 0.37).sin()) } else { 0.0 }
            })
            .collect();
        let mut stft = Stft::new();
        for part in sound.chunks(1000) {
            stft.push(part);
        }
        let spect = stft.finish().unwrap();
        assert_eq!(spect.len() / MELS, 1 + n / HOP);
        let (beat, down) = logits(&spect).unwrap();
        let h = beats_of(&beat, &down);
        assert!(h.beats.len() >= 30, "{:?}", h.beats);
        let gaps: Vec<f64> = h.beats.windows(2).map(|w| w[1] - w[0]).collect();
        let mut sorted = gaps.clone();
        sorted.sort_by(f64::total_cmp);
        assert!((sorted[sorted.len() / 2] - 0.5).abs() < 0.03, "{gaps:?}");
    }
}
