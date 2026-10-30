//! A track's analysis (decision 0035): worked out once from its sound, kept by the pass (the ThinkBook's first pass,
//! then the NAS for new files) and served from there. One decode gives everything:
//! - **Loudness**: EBU R128 integrated loudness, ReplayGain 2.0's track gain (-18 LUFS reference) and the sample peak;
//!   the gating blocks (400 ms every 100 ms) kept as a histogram, so an album's gain is worked out exactly from its
//!   tracks' without decoding them again (`album_gain`).
//! - **Waveform**: 512 bins along the track, each its RMS, 0..=255 of the track's loudest bin (the seekbar's bars).
//! - **Tempo**: taktus over the mono sound, kept only when it is sure enough (the owner: no tempo is better than a
//!   wrong one; beatless and ambient music gets none). Reggae, dub, roots and dancehall get the slow count (the owner):
//!   an estimate above 110 is halved. Dance music is counted between 90 and 180 (the owner): an estimate outside is
//!   halved or doubled into it (on GiantSteps the exact tempo goes from 82 % to 88 % of the sure answers).
//! - **Beats** (decision 0046): where there is a tempo, every beat's time and strength, by taktus's beat tracker at
//!   that tempo; the visuals pulse on them.
//! - **Signals** (decision 0046, amended), only when the visuals ask: the sound in six bands and over the whole track,
//!   every signal the show can be steered by (`signals`).
//! Blocking and single-threaded: the pass runs several at once.

pub mod band_wave;
pub mod flaws;
pub mod spectrum;
pub mod batch;
pub mod signals;

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use symphonia::core::codecs::audio::{AudioCodecId, AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::Timestamp;

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
    /// `WAVEFORM_BINS` bins, each its stretch's RMS (linear), 0..=255 of the track's loudest bin.
    pub waveform: Vec<u8>,
    /// Whether the decoder gave the sound of the whole stream the reader walked (at least `COMPLETE` of it). Some
    /// files decode only in part (Symphonia's FLAC reader takes one frame in eight of some files; an MP3 whose format
    /// changes mid-stream stops decoding there), and then the length, loudness, tempo and waveform describe only that
    /// part. `None`: not known (analysed before this was recorded, or the stream has no time base).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub complete: Option<bool>,
    /// Every beat (decision 0046), at the tempo kept: none (empty) without a tempo. `None`: not worked out (an analysis
    /// from before beats were kept). The catalogue keeps them apart and reads them alone (`Catalogue::beats_of`): an
    /// analysis read from it has `None` here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beats: Option<Beats>,
}

/// A track's beats as kept and served (decision 0046): five bytes a beat, its time in milliseconds from the start of the
/// sound (u32, little-endian), then how strongly it starts (0..=255, 255 for the track's strong beats). Empty for a
/// track without a tempo. In an analysis' JSON line, base64.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Beats(pub Vec<u8>);

impl Beats {
    /// Bytes a beat.
    pub const SIZE: usize = 5;

    fn of(times: &[f64], strengths: &[u8]) -> Self {
        let mut out = Vec::with_capacity(times.len() * Self::SIZE);
        for (t, s) in times.iter().zip(strengths) {
            out.extend_from_slice(&((t.max(0.0) * 1000.0).round() as u32).to_le_bytes());
            out.push(*s);
        }
        Beats(out)
    }

    /// Each beat: its time (seconds) and strength.
    pub fn each(&self) -> impl Iterator<Item = (f64, u8)> + '_ {
        self.0.chunks_exact(Self::SIZE).map(|c| (f64::from(u32::from_le_bytes([c[0], c[1], c[2], c[3]])) / 1000.0, c[4]))
    }
}

impl Serialize for Beats {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use base64::Engine as _;
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for Beats {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        use base64::Engine as _;
        let text = String::deserialize(d)?;
        base64::engine::general_purpose::STANDARD.decode(text).map(Beats).map_err(serde::de::Error::custom)
    }
}

/// Beats heard by Beat this! (decision 0049, amended): each beat's time and the downbeats' (seconds), kept by the
/// analysis in place of its own grid's; each beat's strength is still the onsets' at it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Heard {
    pub beats: Vec<f64>,
    pub downbeats: Vec<f64>,
}

/// The share of the walked stream the decoded sound must cover to count as complete. Below 1 for what decoders
/// rightly drop: an encoder's delay and padding.
const COMPLETE: f64 = 0.97;

/// Whether `frames` decoded frames cover a stream the reader walked to `walked` (in `time_base` units, numerator over
/// denominator seconds) at `rate` frames a second.
fn covered(frames: u64, walked: u64, time_base: Option<(u32, u32)>, rate: u32) -> Option<bool> {
    let (numer, denom) = time_base?;
    let walked_frames = walked as f64 * f64::from(numer) / f64::from(denom) * f64::from(rate);
    (walked_frames > 0.0).then(|| frames as f64 >= COMPLETE * walked_frames)
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
pub(crate) fn after_id3(path: &Path) -> u64 {
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
    /// What the sound is coded in (FLAC, MP3, …).
    codec: AudioCodecId,
    rate: u32,
    channels: usize,
    /// The track's time base (numerator, denominator), which packets' times are in.
    time_base: Option<(u32, u32)>,
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
    let time_base = track.time_base.map(|tb| (tb.numer.get(), tb.denom.get()));
    let decoder = symphonia::default::get_codecs().make_audio_decoder(&params, &AudioDecoderOptions::default())?;
    let rate = params.sample_rate.context("the sample rate is unknown")?;
    let channels = params.channels.as_ref().map(|c| c.count()).context("the channels are unknown")?;
    if channels == 0 || rate < 8_000 {
        bail!("{channels} channels at {rate} Hz");
    }
    Ok(Opened { reader, decoder, track: id, codec: params.codec, rate, channels, time_base })
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

/// A file's sound decoded whole, as a gate looks at it (0.9.8, single-track replacement): its rate, channels and
/// length, how many frames would not decode, what it is coded in and how many bytes its sound is (for its bitrate).
pub struct Walked {
    pub rate: u32,
    pub channels: usize,
    pub frames: u64,
    pub damaged: u32,
    pub codec: AudioCodecId,
    pub bytes: u64,
}

/// Decodes every packet of the file, handing each one's sound (planar, -1..1) to `each`; a damaged frame is counted
/// and skipped, as the analysis skips it.
pub fn walk(path: &Path, mut each: impl FnMut(&[Vec<f32>], u32, usize)) -> Result<Walked> {
    let mut o = open(path)?;
    let mut w = Walked { rate: o.rate, channels: o.channels, frames: 0, damaged: 0, codec: o.codec, bytes: 0 };
    let mut scratch: Vec<Vec<f32>> = Vec::new();
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
        w.bytes += packet.data.len() as u64;
        let decoded = match o.decoder.decode(&packet) {
            Ok(d) => d,
            Err(SymphoniaError::DecodeError(_)) => {
                w.damaged += 1;
                continue;
            }
            Err(e) => return Err(anyhow!("decoding {}: {e}", path.display())),
        };
        let n = decoded.frames();
        if n == 0 {
            continue;
        }
        decoded.copy_to_vecs_planar::<f32>(&mut scratch);
        if scratch.len() != w.channels {
            bail!("the channel count changed mid-stream");
        }
        for p in scratch.iter_mut() {
            p.truncate(n);
        }
        each(&scratch, w.rate, w.channels);
        w.frames += n as u64;
    }
    if w.frames == 0 {
        bail!("no sound in {}", path.display());
    }
    Ok(w)
}

/// A file's sound read from any frame (0.9.9, Tape rewind): opened as the analysis opens it (an MP3 behind a large
/// ID3v2 tag too), moved to a frame, then decoded a packet at a time.
pub(crate) struct Reading(Opened);

impl Reading {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        open(path).map(Self)
    }

    pub(crate) fn rate(&self) -> u32 {
        self.0.rate
    }

    pub(crate) fn channels(&self) -> usize {
        self.0.channels
    }

    /// Moves to the packet holding `frame`; returns the frame that packet starts at, where decoding continues.
    pub(crate) fn seek(&mut self, frame: u64) -> Result<u64> {
        let o = &mut self.0;
        // Packets' times are in the track's time base; most are 1 / the rate.
        let (numer, denom) = o.time_base.map_or((1, u64::from(o.rate)), |(n, d)| (u64::from(n), u64::from(d)));
        let ts = frame * denom / (numer * u64::from(o.rate));
        let to = o.reader.seek(SeekMode::Accurate, SeekTo::Timestamp { ts: Timestamp::from(ts as i64), track_id: o.track })?;
        o.decoder.reset();
        Ok(u64::try_from(to.actual_ts.get()).unwrap_or(0) * numer * u64::from(o.rate) / denom)
    }

    /// The next packet's sound (planar, -1..1) in `planes`; false at the end. A damaged packet is skipped, as the
    /// analysis skips it.
    pub(crate) fn next(&mut self, planes: &mut Vec<Vec<f32>>) -> Result<bool> {
        let o = &mut self.0;
        loop {
            let packet = match o.reader.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => return Ok(false),
                Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(false),
                Err(e) => return Err(anyhow!("reading: {e}")),
            };
            if packet.track_id != o.track {
                continue;
            }
            let decoded = match o.decoder.decode(&packet) {
                Ok(d) => d,
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(e) => return Err(anyhow!("decoding: {e}")),
            };
            let n = decoded.frames();
            if n == 0 {
                continue;
            }
            decoded.copy_to_vecs_planar::<f32>(planes);
            if planes.len() != o.channels {
                bail!("the channel count changed mid-stream");
            }
            for p in planes.iter_mut() {
                p.truncate(n);
            }
            return Ok(true);
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
    Ok(analyse_inner(path, genres, false, None)?.0)
}

/// The analysis and the sound's signals (decision 0046, amended), from one decode: what the visuals ask for.
/// With `heard`, Beat this!'s beats and downbeats are the analysis' and the signals' (decision 0049, amended).
pub fn analyse_with_signals(path: &Path, genres: &[String], heard: Option<&Heard>) -> Result<(Analysis, signals::Signals)> {
    let (a, b) = analyse_inner(path, genres, true, heard)?;
    Ok((a, b.unwrap_or_default()))
}

fn analyse_inner(path: &Path, genres: &[String], with_signals: bool, heard: Option<&Heard>) -> Result<(Analysis, Option<signals::Signals>)> {
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
    // With the signals: the momentary loudness (400 ms) at each hop, the loudness as heard (decision 0046, amended).
    let mut moments: Vec<f32> = Vec::new();
    // The waveform: the mean square per hundredth of a second, binned at the end.
    let centi = (o.rate / 100).max(1) as usize;
    let mut squares: Vec<f32> = Vec::new();
    let (mut square, mut in_centi) = (0f32, 0usize);
    // The tempo: taktus hears the mono sound.
    let mut ear = taktus::Ear::new(o.rate);
    let mut in_signals = with_signals.then(|| signals::SignalEar::new(o.rate));
    let mut onsets: Vec<f32> = Vec::new();
    let mut frames = 0u64;
    // How far the reader walked the stream (the end of its last packet), against which `frames` is measured.
    let mut walked = 0u64;
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
                if with_signals {
                    moments.push(m as f32);
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
        walked = walked.max((packet.pts.get().max(0) as u64).saturating_add(packet.dur.get()));
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
            let (mut sum, mut energy) = (0f32, 0f32);
            for p in &scratch {
                sum += p[i];
                energy += p[i] * p[i];
            }
            let mono = sum / ch as f32;
            if let Some(onset) = ear.hear(mono) {
                onsets.push(onset);
            }
            if let Some(s) = in_signals.as_mut() {
                // The side: half the first two channels' difference (none in mono).
                let side = if ch >= 2 { (scratch[0][i] - scratch[1][i]) * 0.5 } else { 0.0 };
                s.hear(mono, side);
            }
            square += energy / ch as f32;
            in_centi += 1;
            if in_centi == centi {
                squares.push(square / centi as f32);
                (square, in_centi) = (0.0, 0);
            }
        }
        feed(&scratch, &mut r128, &mut pending)?;
        frames += n as u64;
    }
    if in_centi > 0 {
        squares.push(square / in_centi as f32);
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
    let waveform = bin_waveform(&squares);
    let beat = taktus::tempo_of(&onsets).map(|b| (b.bpm(), b.sure, b.catch));
    let (bpm, bpm_confidence) = tempo(beat, genres);
    let beats = match heard {
        Some(h) => Beats::of(&h.beats, &strengths_at(&onsets, &h.beats)),
        None => beats_of(&onsets, bpm),
    };
    let beat_times: Vec<f64> = beats.each().map(|(t, _)| t).collect();
    let analysis = Analysis {
        version: VERSION,
        duration_secs: frames as f64 / f64::from(o.rate),
        loudness_lufs: loudness as f32,
        track_gain_db: (REFERENCE - loudness) as f32,
        track_peak,
        blocks: Loudness::of(&bins),
        bpm,
        bpm_confidence,
        waveform,
        complete: covered(frames, walked, o.time_base, o.rate),
        beats: Some(beats),
    };
    Ok((analysis, in_signals.map(|s| s.finish(&moments, &beat_times, heard.map(|h| h.downbeats.as_slice())))))
}

/// The beats of a track at the tempo kept (none without one, so a slow-counted track's are the slow ones): taktus's
/// beat tracker over its onsets, each beat with its strength (`strengths_at`).
fn beats_of(onsets: &[f32], bpm: Option<f32>) -> Beats {
    let Some(bpm) = bpm.filter(|b| *b > 0.0) else { return Beats::default() };
    let times = taktus::beats_of(onsets, 60.0 / f64::from(bpm));
    Beats::of(&times, &strengths_at(onsets, &times))
}

/// Each beat's strength: the strongest onset at it, against the track's strong beats (the tenth strongest of them and
/// up are 255).
fn strengths_at(onsets: &[f32], times: &[f64]) -> Vec<u8> {
    // The onset at a time: taktus's time is its onset's index from LEAD on, less the ear's lateness (22 ms, two
    // onsets); the strongest within two onsets of that.
    let raw: Vec<f32> = times
        .iter()
        .map(|t| {
            let at = ((t - taktus::LEAD) * taktus::RATE as f64).round() as i64 + 2;
            (at - 2..=at + 2).filter_map(|i| usize::try_from(i).ok().and_then(|i| onsets.get(i))).copied().fold(0.0, f32::max)
        })
        .collect();
    let mut sorted: Vec<f32> = raw.iter().copied().filter(|v| v.is_finite()).collect();
    sorted.sort_by(f32::total_cmp);
    let strong = sorted.get(sorted.len() * 9 / 10).copied().unwrap_or(0.0);
    raw.iter().map(|v| if strong > 0.0 { (v / strong * 255.0).clamp(0.0, 255.0).round() as u8 } else { 255 }).collect()
}

/// A file's sound in mono, a packet at a time (Beat this!'s spectrogram, `app::beat_this`).
pub(crate) struct Mono {
    o: Opened,
    scratch: Vec<Vec<f32>>,
}

impl Mono {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        Ok(Mono { o: open(path)?, scratch: Vec::new() })
    }

    pub(crate) fn rate(&self) -> u32 {
        self.o.rate
    }

    /// The next packet's frames appended to `out`, the channels' mean; false at the end.
    pub(crate) fn read(&mut self, out: &mut Vec<f32>) -> Result<bool> {
        loop {
            let packet = match self.o.reader.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => return Ok(false),
                Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(false),
                Err(e) => return Err(anyhow!("reading: {e}")),
            };
            if packet.track_id != self.o.track {
                continue;
            }
            let decoded = match self.o.decoder.decode(&packet) {
                Ok(d) => d,
                // A damaged frame: silence as long as it, so what follows keeps its time (as ffmpeg conceals it).
                Err(SymphoniaError::DecodeError(_)) => {
                    let (numer, denom) = self.o.time_base.unwrap_or((1, self.o.rate));
                    let frames = packet.dur.get().saturating_mul(u64::from(numer)).saturating_mul(u64::from(self.o.rate)) / u64::from(denom.max(1));
                    if frames == 0 || frames > u64::from(self.o.rate) {
                        continue;
                    }
                    out.extend(std::iter::repeat_n(0.0, frames as usize));
                    return Ok(true);
                }
                Err(e) => return Err(anyhow!("decoding: {e}")),
            };
            let n = decoded.frames();
            if n == 0 {
                continue;
            }
            decoded.copy_to_vecs_planar::<f32>(&mut self.scratch);
            let ch = self.scratch.len().max(1);
            out.extend((0..n).map(|i| self.scratch.iter().map(|p| p.get(i).copied().unwrap_or(0.0)).sum::<f32>() / ch as f32));
            return Ok(true);
        }
    }
}

/// The hundredths' mean squares in `WAVEFORM_BINS` bins, each its RMS, as 0..=255 of the loudest bin's. RMS, not the
/// peaks: a loud master's peaks are near full scale all through, so its waveform would be a flat block (decision 0035).
/// Against the track's own loudest, so a quiet track keeps its detail in a byte (its loudness is kept apart).
fn bin_waveform(squares: &[f32]) -> Vec<u8> {
    if squares.is_empty() {
        return vec![0; WAVEFORM_BINS];
    }
    let rms: Vec<f32> = (0..WAVEFORM_BINS)
        .map(|b| {
            let from = (b * squares.len() / WAVEFORM_BINS).min(squares.len() - 1);
            let to = ((b + 1) * squares.len() / WAVEFORM_BINS).max(from + 1).min(squares.len());
            (squares[from..to].iter().sum::<f32>() / (to - from) as f32).sqrt()
        })
        .collect();
    let top = rms.iter().copied().fold(0.0, f32::max);
    if top <= 0.0 {
        return vec![0; WAVEFORM_BINS];
    }
    rms.iter().map(|r| (r / top * 255.0).round() as u8).collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn complete_is_measured_against_what_the_reader_walked() {
        // The two cases found in the library (2026-10-17), in 1/44100 s units: a FLAC whose reader walked 5,999,364
        // frames while 732,672 decoded, an MP3 walked to 23,115,311 and decoded to 9,361,199; and a whole decode.
        let cd = Some((1, 44_100));
        assert_eq!(covered(732_672, 5_999_364, cd, 44_100), Some(false));
        assert_eq!(covered(9_361_199, 23_115_311, cd, 44_100), Some(false));
        assert_eq!(covered(5_999_364, 5_999_364, cd, 44_100), Some(true));
        // An encoder's delay and padding are not missing sound.
        assert_eq!(covered(5_999_364 - 2_000, 5_999_364, cd, 44_100), Some(true));
        // Another time base: milliseconds, at 48 kHz.
        assert_eq!(covered(48_000 * 200, 200_000, Some((1, 1_000)), 48_000), Some(true));
        assert_eq!(covered(48_000 * 100, 200_000, Some((1, 1_000)), 48_000), Some(false));
        // Not known without a time base or a walk.
        assert_eq!(covered(100, 1_000, None, 44_100), None);
        assert_eq!(covered(100, 0, cd, 44_100), None);
    }

    #[test]
    fn an_analysis_from_before_complete_was_recorded_reads_as_not_known() {
        let mut a = analyse(&wav("old-line", 1.0, |t| 0.2 * (2.0 * std::f64::consts::PI * 440.0 * t).sin() as f32), &[]).unwrap();
        let line = serde_json::to_string(&Analysis { complete: None, ..a.clone() }).unwrap();
        assert!(!line.contains("complete"), "an unknown is left out of a line");
        a.complete = None;
        assert_eq!(serde_json::from_str::<Analysis>(&line).unwrap(), a);
    }

    /// A 16-bit stereo WAV of `secs` seconds at 44.1 kHz, each sample from `f(t)`.
    pub(crate) fn wav(name: &str, secs: f64, f: impl Fn(f64) -> f32) -> std::path::PathBuf {
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
        assert_eq!(a.complete, Some(true), "a whole decode");
        assert!((a.loudness_lufs + 20.0).abs() < 0.5, "{}", a.loudness_lufs);
        assert!((a.track_gain_db - (-18.0 - a.loudness_lufs)).abs() < 1e-4);
        assert!((a.track_peak - 0.1).abs() < 0.005, "{}", a.track_peak);
        assert_eq!(a.waveform.len(), WAVEFORM_BINS);
        assert!(a.waveform.iter().all(|&w| w >= 253), "a steady tone is flat at the top: {:?}", &a.waveform[..8]);
        assert_eq!(a.bpm, None, "a steady tone has no beat");
        // The blocks give the same loudness as the whole (one track's album gain is its track gain).
        let (gain, peak) = album_gain(&[&a]).unwrap();
        assert!((gain - a.track_gain_db).abs() < 0.15, "{gain} against {}", a.track_gain_db);
        assert!((peak - a.track_peak).abs() < 1e-6);
    }

    #[test]
    fn the_waveform_follows_the_loudness_not_the_peaks() {
        // 3 s of a sine at 0.4, then 3 s of a square wave at 0.1: the square's peaks are a quarter of the sine's, its RMS
        // a third of it (0.1 against 0.4 / sqrt 2), which the waveform shows.
        let path = wav("steps", 6.0, |t| {
            let s = (2.0 * std::f64::consts::PI * 440.0 * t).sin() as f32;
            if t < 3.0 { 0.4 * s } else { 0.1 * s.signum() }
        });
        let a = analyse(&path, &[]).unwrap();
        std::fs::remove_file(&path).ok();
        let (first, second) = a.waveform.split_at(WAVEFORM_BINS / 2);
        // A bin is not a whole number of the sine's periods: its RMS wobbles by about 1 %.
        assert!(first[..250].iter().all(|&w| w >= 248), "{:?}", &first[..8]);
        let want = (0.1 / (0.4 / 2f32.sqrt()) * 255.0) as u8;
        assert!(second[6..].iter().all(|&w| w.abs_diff(want) <= 2), "{want}: {:?}", &second[6..14]);
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

    /// Short noise bursts at `clicks` (seconds), each `loud(k)` (0..=1), over `secs` seconds.
    pub(crate) fn click_track(name: &str, secs: f64, clicks: &[f64], loud: impl Fn(usize) -> f32) -> std::path::PathBuf {
        let clicks = clicks.to_vec();
        wav(name, secs, move |t| {
            let k = clicks.partition_point(|c| *c <= t);
            if k == 0 {
                return 0.0;
            }
            let into = t - clicks[k - 1];
            if into >= 0.02 {
                return 0.0;
            }
            (((t * 12_345.678).sin() * 43_758.545).fract() as f32 - 0.5) * 1.6 * (1.0 - into as f32 / 0.02) * loud(k - 1)
        })
    }

    /// How far each beat is from the nearest click (seconds), and how many clicks have a beat within `near` of them.
    fn against(beats: &Beats, clicks: &[f64], near: f64) -> (Vec<f64>, usize) {
        let off: Vec<f64> = beats.each().map(|(b, _)| clicks.iter().map(|c| (b - c).abs()).fold(f64::MAX, f64::min)).collect();
        let caught = clicks.iter().filter(|c| beats.each().any(|(b, _)| (b - *c).abs() <= near)).count();
        (off, caught)
    }

    #[test]
    fn the_beats_fall_on_the_clicks() {
        // 120 BPM from 0.25 s on, for 30 s.
        let clicks: Vec<f64> = (0..60).map(|k| 0.25 + 0.5 * k as f64).collect();
        let path = click_track("beats", 30.0, &clicks, |_| 1.0);
        let a = analyse(&path, &[]).unwrap();
        std::fs::remove_file(&path).ok();
        let beats = a.beats.clone().expect("worked out");
        let (off, caught) = against(&beats, &clicks, 0.015);
        assert!(off.iter().all(|o| *o <= 0.015), "every beat within 15 ms of a click: {off:?}");
        assert!(caught >= 57, "{caught} of 60 clicks have their beat");
        assert!(beats.each().all(|(_, s)| s >= 200), "even clicks are all strong: {:?}", beats.each().map(|b| b.1).collect::<Vec<_>>());
        // It goes into an analysis' line and back.
        let line = serde_json::to_string(&Analysis { beats: Some(beats.clone()), ..a.clone() }).unwrap();
        assert_eq!(serde_json::from_str::<Analysis>(&line).unwrap().beats, Some(beats));
    }

    #[test]
    fn the_loudness_as_heard_follows_the_sound_every_tenth_of_a_second() {
        // 3 s of a 1 kHz sine at 0.1 (about -20 LUFS, as above), then 3 s at 0.01 (20 dB down).
        let path = wav("moments", 6.0, |t| if t < 3.0 { 0.1 } else { 0.01 } * (2.0 * std::f64::consts::PI * 1000.0 * t).sin() as f32);
        let (a, signals) = analyse_with_signals(&path, &[], None).unwrap();
        std::fs::remove_file(&path).ok();
        let moments = signals.loudness().expect("reads back");
        assert!(moments.len().abs_diff(60) <= 1, "ten a second: {}", moments.len());
        let around = |secs: f64| moments[(secs * signals::MOMENTS_RATE as f64) as usize];
        assert!((around(1.5) + 20.0).abs() < 0.7, "{}", around(1.5));
        assert!((around(4.5) + 40.0).abs() < 0.7, "{}", around(4.5));
        // A window by its middle: 200 ms either side of the step it is half way between the two.
        assert!(around(2.8) > -21.0 && around(3.2) < -39.0, "{} {}", around(2.8), around(3.2));
        assert!((a.loudness_lufs - moments[15]).abs() < 4.0, "the same scale as the integrated loudness");
    }

    #[test]
    fn no_tempo_no_beats() {
        let path = wav("no-beats", 10.0, |t| 0.1 * (2.0 * std::f64::consts::PI * 1000.0 * t).sin() as f32);
        let a = analyse(&path, &[]).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!((a.bpm, a.beats), (None, Some(Beats::default())));
    }

    #[test]
    fn after_a_silent_intro_the_beats_start_with_the_music() {
        // 8 s of nothing, then 128 BPM for 30 s.
        let clicks: Vec<f64> = (0..64).map(|k| 8.0 + 60.0 / 128.0 * k as f64).collect();
        let path = click_track("intro", 38.0, &clicks, |_| 1.0);
        let a = analyse(&path, &[]).unwrap();
        std::fs::remove_file(&path).ok();
        let beats = a.beats.expect("worked out");
        assert!(beats.each().all(|(b, _)| b > 7.9), "no beat in the silence: {:?}", beats.each().take(4).collect::<Vec<_>>());
        let (off, caught) = against(&beats, &clicks, 0.015);
        assert!(off.iter().all(|o| *o <= 0.015), "{off:?}");
        assert!(caught >= 60, "{caught} of 64");
    }

    #[test]
    fn the_beats_follow_a_tempo_that_drifts() {
        // 120 BPM speeding up evenly to 126 over 40 s, as a band does.
        let mut clicks = vec![0.3];
        while *clicks.last().unwrap() < 39.5 {
            let t = *clicks.last().unwrap();
            clicks.push(t + 60.0 / (120.0 + 6.0 * t / 40.0));
        }
        let path = click_track("drift", 40.0, &clicks, |_| 1.0);
        let a = analyse(&path, &[]).unwrap();
        std::fs::remove_file(&path).ok();
        let beats = a.beats.expect("worked out");
        let (off, caught) = against(&beats, &clicks, 0.015);
        assert!(off.iter().all(|o| *o <= 0.015), "{off:?}");
        assert!(caught * 100 >= clicks.len() * 95, "{caught} of {}", clicks.len());
    }

    #[test]
    fn an_accented_beat_is_stronger() {
        // 120 BPM, every fourth click at full strength, the others at a quarter.
        let clicks: Vec<f64> = (0..60).map(|k| 0.25 + 0.5 * k as f64).collect();
        let path = click_track("accents", 30.0, &clicks, |k| if k % 4 == 0 { 1.0 } else { 0.25 });
        let a = analyse(&path, &[]).unwrap();
        std::fs::remove_file(&path).ok();
        let beats = a.beats.expect("worked out");
        let strength = |accented: bool| {
            let s: Vec<f64> = beats
                .each()
                .filter(|(b, _)| clicks.iter().position(|c| (b - c).abs() <= 0.015).is_some_and(|k| (k % 4 == 0) == accented))
                .map(|(_, s)| f64::from(s))
                .collect();
            s.iter().sum::<f64>() / s.len().max(1) as f64
        };
        assert!(strength(true) > strength(false) + 30.0, "{} against {}", strength(true), strength(false));
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
