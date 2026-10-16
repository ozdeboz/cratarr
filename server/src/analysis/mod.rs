//! A track's analysis (decision 0035): worked out once from its sound, kept by the pass (the ThinkBook's first pass,
//! then the NAS for new files) and served from there. One decode gives everything:
//! - **Loudness**: EBU R128 integrated loudness, ReplayGain 2.0's track gain (-18 LUFS reference) and the sample peak;
//!   the gating blocks (400 ms every 100 ms) kept as a histogram, so an album's gain is worked out exactly from its
//!   tracks' without decoding them again (`album_gain`).
//! - **Waveform**: 512 bins along the track, each its loudest sample, 0..=255 of full scale (the seekbar's bars).
//! - **Tempo**: taktus over the mono sound, kept only when it is sure enough (the owner: no tempo is better than a
//!   wrong one; beatless and ambient music gets none). Reggae, dub, roots and dancehall get the slow count (the owner):
//!   an estimate above 110 is halved. Dance music is counted between 90 and 180 (the owner): an estimate outside is
//!   halved or doubled into it (on GiantSteps the exact tempo goes from 82 % to 88 % of the sure answers).
//! Blocking and single-threaded: the pass runs several at once.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;

/// The algorithm's version: a result of another version is worked out again.
pub const VERSION: u32 = 1;
/// The waveform's bins.
pub const WAVEFORM_BINS: usize = 512;

/// The loudness histogram's range and step (LUFS): blocks below the bottom are R128's absolute gate.
const LOWEST: f64 = -70.0;
const HIGHEST: f64 = 5.0;
const STEP: f64 = 0.1;
const BINS: usize = ((HIGHEST - LOWEST) / STEP) as usize;
/// ReplayGain 2.0's reference loudness.
const REFERENCE: f64 = -18.0;

/// taktus's confidence gate: a tempo is kept only when both hold (measured on Ballroom and GiantSteps, decision 0035).
const SURE: f64 = 0.05;
const CATCH: f64 = 1.0;
/// Genres counted slow (the owner): an estimate above `SLOW_ABOVE` is halved.
const SLOW_GENRES: [&str; 6] = ["reggae", "dub", "roots", "roots reggae", "dub reggae", "dancehall"];
const SLOW_ABOVE: f32 = 110.0;
/// Words that make a genre tag dance music (whole words of the tag), counted between `DANCE_FROM` and twice it.
const DANCE_WORDS: [&str; 16] =
    ["house", "techno", "trance", "electro", "disco", "garage", "breakbeat", "breaks", "dnb", "jungle", "dubstep", "hardstyle", "edm", "dance", "idm-dance", "footwork"];
const DANCE_PHRASES: [&str; 3] = ["drum and bass", "drum & bass", "drum'n'bass"];
const DANCE_FROM: f32 = 90.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    pub version: u32,
    /// The decoded length.
    pub duration_secs: f64,
    /// EBU R128 integrated loudness (LUFS); -70 for silence.
    pub loudness_lufs: f32,
    /// ReplayGain 2.0: -18 - loudness.
    pub track_gain_db: f32,
    /// The loudest sample, linear (1.0 is full scale; a lossy file can go over).
    pub track_peak: f32,
    /// The gating blocks' loudness, for an album's gain.
    pub blocks: Loudness,
    /// None when the estimate is not sure enough (or there is no beat).
    pub bpm: Option<f32>,
    /// The estimator's own confidence (taktus's `sure`), 0..=1.
    pub bpm_confidence: f32,
    /// `WAVEFORM_BINS` bins, each its loudest sample, 0..=255 of full scale.
    pub waveform: Vec<u8>,
}

/// Gating blocks by loudness: (bin, count), bin `i` covering LOWEST + i * 0.1 LUFS; only bins that hold any.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Loudness(pub Vec<(u16, u32)>);

impl Loudness {
    fn of(bins: &[u32]) -> Self {
        Loudness(bins.iter().enumerate().filter(|(_, n)| **n > 0).map(|(i, n)| (i as u16, *n)).collect())
    }

    /// R128's integrated loudness over these blocks: the absolute gate (-70) is the histogram's bottom; the relative
    /// gate is 10 LU under the mean of what passed it. None without any block.
    fn integrated(merged: &[u32]) -> Option<f64> {
        let energy = |i: usize| 10f64.powf((LOWEST + (i as f64 + 0.5) * STEP + 0.691) / 10.0);
        let mean = |from: usize| {
            let (sum, n) = merged.iter().enumerate().skip(from).fold((0.0, 0u64), |(s, n), (i, c)| (s + energy(i) * f64::from(*c), n + u64::from(*c)));
            (n > 0).then(|| sum / n as f64)
        };
        let ungated = mean(0)?;
        let relative = -0.691 + 10.0 * ungated.log10() - 10.0;
        let from = (((relative - LOWEST) / STEP).floor().max(0.0) as usize).min(BINS);
        mean(from).map(|m| -0.691 + 10.0 * m.log10())
    }
}

/// An album's gain and peak from its tracks' analyses (ReplayGain 2.0's album gain: R128 over all their blocks).
pub fn album_gain(tracks: &[&Analysis]) -> Option<(f32, f32)> {
    let mut merged = vec![0u32; BINS];
    for t in tracks {
        for &(i, n) in &t.blocks.0 {
            if let Some(b) = merged.get_mut(usize::from(i)) {
                *b += n;
            }
        }
    }
    let loudness = Loudness::integrated(&merged)?;
    let peak = tracks.iter().map(|t| t.track_peak).fold(0.0, f32::max);
    Some(((REFERENCE - loudness) as f32, peak))
}

/// The file from `start` on, as if that were its beginning: an MP3 whose ID3v2 tag (a large cover, say) keeps the probe
/// from finding its audio (decision 0035's spike: 13 % of the library's MP3s).
struct After {
    file: File,
    start: u64,
    len: u64,
}

impl Read for After {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buf)
    }
}

impl Seek for After {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let to = match pos {
            SeekFrom::Start(n) => SeekFrom::Start(self.start + n),
            other => other,
        };
        Ok(self.file.seek(to)?.saturating_sub(self.start))
    }
}

impl MediaSource for After {
    fn is_seekable(&self) -> bool {
        true
    }
    fn byte_len(&self) -> Option<u64> {
        Some(self.len)
    }
}

/// Where the audio starts after an ID3v2 tag (its header's syncsafe size, and a footer when flagged); 0 without one.
fn after_id3(path: &Path) -> u64 {
    let mut h = [0u8; 10];
    let Ok(mut f) = File::open(path) else { return 0 };
    if f.read_exact(&mut h).is_err() || &h[..3] != b"ID3" {
        return 0;
    }
    let size = h[6..10].iter().fold(0u64, |a, &b| (a << 7) | u64::from(b & 0x7f));
    10 + size + if h[5] & 0x10 != 0 { 10 } else { 0 }
}

struct Opened {
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track: u32,
    rate: u32,
    channels: usize,
}

fn open_from(path: &Path, start: u64) -> Result<Opened> {
    let mut file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let len = file.metadata()?.len().saturating_sub(start);
    file.seek(SeekFrom::Start(start))?;
    let mss = MediaSourceStream::new(Box::new(After { file, start, len }), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let reader = symphonia::default::get_probe().probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())?;
    let track = reader.default_track(TrackType::Audio).context("no audio track")?;
    let params = track.codec_params.as_ref().and_then(|p| p.audio()).cloned().context("no audio parameters")?;
    let id = track.id;
    let decoder = symphonia::default::get_codecs().make_audio_decoder(&params, &AudioDecoderOptions::default())?;
    let rate = params.sample_rate.context("the sample rate is unknown")?;
    let channels = params.channels.as_ref().map(|c| c.count()).context("the channels are unknown")?;
    if channels == 0 || rate < 8_000 {
        bail!("{channels} channels at {rate} Hz");
    }
    Ok(Opened { reader, decoder, track: id, rate, channels })
}

/// Opens the file; one that does not probe is tried again after its ID3v2 tag.
fn open(path: &Path) -> Result<Opened> {
    match open_from(path, 0) {
        Ok(o) => Ok(o),
        Err(e) => {
            let start = after_id3(path);
            if start == 0 {
                return Err(e);
            }
            open_from(path, start).map_err(|e2| anyhow!("{e:#}; after the ID3 tag too: {e2:#}"))
        }
    }
}

/// Whether a track of these genres is counted slow.
fn slow(genres: &[String]) -> bool {
    genres.iter().any(|g| SLOW_GENRES.contains(&g.trim().to_lowercase().as_str()))
}

/// Whether a track of these genres is dance music (and not counted slow).
fn dance(genres: &[String]) -> bool {
    genres.iter().any(|g| {
        let g = g.trim().to_lowercase();
        DANCE_PHRASES.iter().any(|p| g.contains(p)) || g.split(|c: char| !c.is_alphanumeric() && c != '-').any(|w| DANCE_WORDS.contains(&w))
    })
}

/// Into [from, 2 * from), by halving or doubling.
fn fold(bpm: f32, from: f32) -> f32 {
    let mut b = bpm;
    while b < from {
        b *= 2.0;
    }
    while b >= 2.0 * from {
        b /= 2.0;
    }
    b
}

/// The tempo the owner keeps: sure enough; halved for the slow genres, folded into 90-180 for dance music.
fn tempo(beat: Option<(f64, f64, f64)>, genres: &[String]) -> (Option<f32>, f32) {
    let Some((bpm, sure, catch)) = beat else { return (None, 0.0) };
    let confidence = sure.clamp(0.0, 1.0) as f32;
    if sure < SURE || catch < CATCH || !(bpm.is_finite() && bpm > 0.0) {
        return (None, confidence);
    }
    let bpm = bpm as f32;
    let bpm = if slow(genres) {
        if bpm > SLOW_ABOVE { bpm / 2.0 } else { bpm }
    } else if dance(genres) {
        fold(bpm, DANCE_FROM)
    } else {
        bpm
    };
    (Some(bpm), confidence)
}

/// One track's analysis from its file. `genres`: its genre tags (for the slow count).
pub fn analyse(path: &Path, genres: &[String]) -> Result<Analysis> {
    let mut o = open(path)?;
    let ch = o.channels;
    let mut r128 = ebur128::EbuR128::new(ch as u32, o.rate, ebur128::Mode::I | ebur128::Mode::M | ebur128::Mode::SAMPLE_PEAK)
        .map_err(|e| anyhow!("loudness: {e}"))?;
    // R128's gating blocks: 400 ms, every 100 ms. The sound goes in a hop at a time, and after each one (from the
    // fourth on) the momentary loudness is the block just completed.
    let hop = (o.rate / 10) as usize;
    let mut pending: Vec<Vec<f32>> = vec![Vec::new(); ch];
    let mut hops = 0u64;
    let mut bins = vec![0u32; BINS];
    // The waveform: the loudest sample per hundredth of a second, binned at the end.
    let centi = (o.rate / 100).max(1) as usize;
    let mut peaks: Vec<f32> = Vec::new();
    let (mut peak, mut in_centi) = (0f32, 0usize);
    // The tempo: taktus hears the mono sound.
    let mut ear = taktus::Ear::new(o.rate);
    let mut onsets: Vec<f32> = Vec::new();
    let mut frames = 0u64;
    let mut scratch: Vec<Vec<f32>> = Vec::new();

    let mut feed = |planes: &[Vec<f32>], r128: &mut ebur128::EbuR128, pending: &mut Vec<Vec<f32>>| -> Result<()> {
        for (p, s) in pending.iter_mut().zip(planes) {
            p.extend_from_slice(s);
        }
        while pending[0].len() >= hop {
            let slices: Vec<&[f32]> = pending.iter().map(|p| &p[..hop]).collect();
            r128.add_frames_planar_f32(&slices).map_err(|e| anyhow!("loudness: {e}"))?;
            hops += 1;
            if hops >= 4 {
                let m = r128.loudness_momentary().unwrap_or(f64::NEG_INFINITY);
                if m.is_finite() && m >= LOWEST {
                    let i = (((m - LOWEST) / STEP) as usize).min(BINS - 1);
                    bins[i] += 1;
                }
            }
            for p in pending.iter_mut() {
                p.drain(..hop);
            }
        }
        Ok(())
    };

    loop {
        let packet = match o.reader.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(anyhow!("reading {}: {e}", path.display())),
        };
        if packet.track_id != o.track {
            continue;
        }
        let decoded = match o.decoder.decode(&packet) {
            Ok(d) => d,
            // A damaged frame of a lossy file: skipped, as a player skips it.
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(anyhow!("decoding {}: {e}", path.display())),
        };
        let n = decoded.frames();
        if n == 0 {
            continue;
        }
        decoded.copy_to_vecs_planar::<f32>(&mut scratch);
        if scratch.len() != ch {
            bail!("the channel count changed mid-stream");
        }
        for p in scratch.iter_mut() {
            p.truncate(n);
        }
        for i in 0..n {
            let (mut sum, mut loud) = (0f32, 0f32);
            for p in &scratch {
                sum += p[i];
                loud = loud.max(p[i].abs());
            }
            if let Some(onset) = ear.hear(sum / ch as f32) {
                onsets.push(onset);
            }
            peak = peak.max(loud);
            in_centi += 1;
            if in_centi == centi {
                peaks.push(peak);
                (peak, in_centi) = (0.0, 0);
            }
        }
        feed(&scratch, &mut r128, &mut pending)?;
        frames += n as u64;
    }
    if in_centi > 0 {
        peaks.push(peak);
    }
    // What is left (under a hop) still counts for the integrated loudness and the peak, not for a block.
    if !pending[0].is_empty() {
        let slices: Vec<&[f32]> = pending.iter().map(|p| p.as_slice()).collect();
        r128.add_frames_planar_f32(&slices).map_err(|e| anyhow!("loudness: {e}"))?;
    }
    if frames == 0 {
        bail!("no sound in {}", path.display());
    }

    let loudness = r128.loudness_global().ok().filter(|l| l.is_finite()).unwrap_or(LOWEST).max(LOWEST);
    let track_peak = (0..ch as u32).filter_map(|c| r128.sample_peak(c).ok()).fold(0.0, f64::max) as f32;
    let waveform = bin_waveform(&peaks);
    let beat = taktus::tempo_of(&onsets).map(|b| (b.bpm(), b.sure, b.catch));
    let (bpm, bpm_confidence) = tempo(beat, genres);
    Ok(Analysis {
        version: VERSION,
        duration_secs: frames as f64 / f64::from(o.rate),
        loudness_lufs: loudness as f32,
        track_gain_db: (REFERENCE - loudness) as f32,
        track_peak,
        blocks: Loudness::of(&bins),
        bpm,
        bpm_confidence,
        waveform,
    })
}

/// The hundredths' peaks in `WAVEFORM_BINS` bins, each its loudest, as 0..=255 of full scale.
fn bin_waveform(peaks: &[f32]) -> Vec<u8> {
    if peaks.is_empty() {
        return vec![0; WAVEFORM_BINS];
    }
    (0..WAVEFORM_BINS)
        .map(|b| {
            let from = b * peaks.len() / WAVEFORM_BINS;
            let to = ((b + 1) * peaks.len() / WAVEFORM_BINS).max(from + 1).min(peaks.len());
            let p = peaks[from.min(peaks.len() - 1)..to].iter().copied().fold(0.0, f32::max);
            (p.min(1.0) * 255.0).round() as u8
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 16-bit stereo WAV of `secs` seconds at 44.1 kHz, each sample from `f(t)`.
    fn wav(name: &str, secs: f64, f: impl Fn(f64) -> f32) -> std::path::PathBuf {
        let rate = 44_100u32;
        let n = (secs * f64::from(rate)) as usize;
        let mut data = Vec::with_capacity(n * 4);
        for i in 0..n {
            let s = (f(i as f64 / f64::from(rate)).clamp(-1.0, 1.0) * 32767.0) as i16;
            data.extend_from_slice(&s.to_le_bytes());
            data.extend_from_slice(&s.to_le_bytes());
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * 4).to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&data);
        let path = std::env::temp_dir().join(format!("cratarr-analysis-{}-{name}.wav", std::process::id()));
        std::fs::write(&path, out).unwrap();
        path
    }

    #[test]
    fn a_sine_has_its_loudness_peak_and_flat_waveform_and_no_tempo() {
        // A 1 kHz sine at 0.1 (-20 dBFS) in both channels: R128 gives about -20 LUFS (+3 for two channels, -3 for a
        // sine's RMS against its peak, nearly nothing from the K filter at 1 kHz).
        let path = wav("sine", 10.0, |t| 0.1 * (2.0 * std::f64::consts::PI * 1000.0 * t).sin() as f32);
        let a = analyse(&path, &[]).unwrap();
        std::fs::remove_file(&path).ok();
        assert!((a.duration_secs - 10.0).abs() < 0.01, "{}", a.duration_secs);
        assert!((a.loudness_lufs + 20.0).abs() < 0.5, "{}", a.loudness_lufs);
        assert!((a.track_gain_db - (-18.0 - a.loudness_lufs)).abs() < 1e-4);
        assert!((a.track_peak - 0.1).abs() < 0.005, "{}", a.track_peak);
        assert_eq!(a.waveform.len(), WAVEFORM_BINS);
        assert!(a.waveform.iter().all(|&w| (24..=27).contains(&w)), "{:?}", &a.waveform[..8]);
        assert_eq!(a.bpm, None, "a steady tone has no beat");
        // The blocks give the same loudness as the whole (one track's album gain is its track gain).
        let (gain, peak) = album_gain(&[&a]).unwrap();
        assert!((gain - a.track_gain_db).abs() < 0.15, "{gain} against {}", a.track_gain_db);
        assert!((peak - a.track_peak).abs() < 1e-6);
    }

    #[test]
    fn clicks_at_120_have_a_tempo_and_reggae_gets_the_slow_count() {
        // Short noise bursts every 0.5 s for 30 s: 120 BPM.
        let path = wav("clicks", 30.0, |t| {
            let into = t % 0.5;
            if into < 0.02 { (((t * 12_345.678).sin() * 43_758.545).fract() as f32 - 0.5) * 1.6 * (1.0 - into as f32 / 0.02) } else { 0.0 }
        });
        let a = analyse(&path, &[]).unwrap();
        let b = analyse(&path, &["Reggae".to_string()]).unwrap();
        std::fs::remove_file(&path).ok();
        let bpm = a.bpm.expect("a tempo for a steady click");
        assert!((bpm - 120.0).abs() < 2.0, "{bpm}");
        assert!((b.bpm.unwrap() - 60.0).abs() < 1.0, "reggae above 110 is halved: {:?}", b.bpm);
    }

    #[test]
    fn an_album_of_a_loud_and_a_quiet_track_sits_between_them() {
        let tone = |amp: f32| move |t: f64| amp * (2.0 * std::f64::consts::PI * 440.0 * t).sin() as f32;
        let (p1, p2) = (wav("loud", 6.0, tone(0.4)), wav("quiet", 6.0, tone(0.05)));
        let (loud, quiet) = (analyse(&p1, &[]).unwrap(), analyse(&p2, &[]).unwrap());
        std::fs::remove_file(&p1).ok();
        std::fs::remove_file(&p2).ok();
        assert!((loud.loudness_lufs - quiet.loudness_lufs - 18.06).abs() < 0.3, "{} {}", loud.loudness_lufs, quiet.loudness_lufs);
        let (gain, peak) = album_gain(&[&loud, &quiet]).unwrap();
        // The quiet half (18 LU down) falls under the relative gate (10 LU under the mean, which the loud half sets at
        // about 3 under itself): the album sounds as the loud track.
        assert!((gain - loud.track_gain_db).abs() < 0.2, "{gain} against {}", loud.track_gain_db);
        assert!((peak - 0.4).abs() < 0.01);
        let json = serde_json::to_string(&loud.blocks).unwrap();
        assert!(json.starts_with("[["), "{json}");
    }

    #[test]
    fn slow_genres_are_whole_tags() {
        assert!(slow(&["Dub".into()]) && slow(&[" roots reggae ".into()]) && slow(&["Dancehall".into()]));
        assert!(!slow(&["Dub Techno".into()]) && !slow(&["Dubstep".into()]) && !slow(&["Techno".into()]));
        assert_eq!(tempo(Some((140.0, 0.01, 3.0)), &[]), (None, 0.01), "not sure enough");
        assert_eq!(tempo(Some((140.0, 0.3, 0.5)), &[]).0, None, "no catch");
        assert_eq!(tempo(Some((100.0, 0.3, 2.0)), &["Reggae".into()]).0, Some(100.0), "at or under 110 stays");
    }

    #[test]
    fn dance_music_is_counted_between_90_and_180() {
        let t = |bpm: f64, genre: &str| tempo(Some((bpm, 0.3, 2.0)), &[genre.to_string()]).0.unwrap();
        assert_eq!(t(62.0, "Deep House"), 124.0);
        assert_eq!(t(248.0, "Techno"), 124.0);
        assert_eq!(t(87.0, "Drum & Bass"), 174.0);
        assert_eq!(t(70.0, "Dubstep"), 140.0);
        assert_eq!(t(128.0, "Electro House"), 128.0, "already within");
        assert_eq!(t(85.0, "Downtempo"), 85.0, "not dance");
        assert_eq!(t(85.0, "Electronic"), 85.0, "too broad to fold");
        assert_eq!(t(150.0, "Dancehall"), 75.0, "the slow count wins");
        assert!(dance(&["Tech House".into()]) && dance(&["UK Garage".into()]) && dance(&["Nu Disco".into()]) && !dance(&["Ambient".into()]));
    }
}
