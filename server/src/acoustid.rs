//! A file's fingerprint and what AcoustID says it is (0.9.8, single-track replacement): the gate a downloaded track
//! passes before it goes into an album. The fingerprint is Chromaprint's (rusty-chromaprint, MIT, on Symphonia's
//! decode, so no fpcalc binary), over the first two minutes as fpcalc takes it; the whole file is decoded too, a frame
//! that will not decode is counted, and the same decode is heard for flaws (`analysis::flaws`: highs that stop at a
//! lossy encoder's wall, clicks, a pop at the end, clipping). AcoustID's lookup (an application key the owner types
//! into Settings) names the MusicBrainz recordings the fingerprint is of; the file passes when one of them is the
//! recording asked for, at a score of at least [`SURE`].

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use base64::Engine;
use rusty_chromaprint::{Configuration, FingerprintCompressor, Fingerprinter};
use serde::Deserialize;

use crate::analysis::flaws::{Findings, Flaws, Sold};

/// How much of the sound the fingerprint hears (fpcalc's own length).
const HEARD_SECS: u64 = 120;
/// The lowest AcoustID score that counts as the recording (the owner's gate, 2026-10-26).
pub const SURE: f32 = 0.9;

/// A file as the gate sees it.
#[derive(Debug, Clone)]
pub struct Print {
    /// Chromaprint's compressed fingerprint, in its own base64 (URL-safe, no padding), as AcoustID takes it.
    pub fingerprint: String,
    pub duration_secs: f64,
    /// Frames that would not decode.
    pub damaged: u32,
    /// What its sound was heard to have wrong with it, if anything.
    pub flaws: Findings,
}

/// Decodes the whole file, hears all of it for flaws and fingerprints its start. Blocking.
pub fn print(path: &Path) -> Result<Print> {
    let config = Configuration::preset_test2();
    let mut printer = Fingerprinter::new(&config);
    let mut started = false;
    let mut heard = 0u64;
    let mut interleaved: Vec<i16> = Vec::new();
    let mut failed = None;
    let mut flaws: Option<Flaws> = None;
    let walked = crate::analysis::walk(path, |planes, rate, channels| {
        flaws.get_or_insert_with(|| Flaws::new(rate)).hear(planes);
        if failed.is_some() || heard >= HEARD_SECS * u64::from(rate) {
            return;
        }
        if !started {
            if let Err(e) = printer.start(rate, channels as u32) {
                failed = Some(format!("{e:?}"));
                return;
            }
            started = true;
        }
        let n = planes[0].len();
        interleaved.clear();
        interleaved.reserve(n * channels);
        for i in 0..n {
            for p in planes {
                interleaved.push((p[i].clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16);
            }
        }
        printer.consume(&interleaved);
        heard += n as u64;
    })?;
    if let Some(e) = failed {
        bail!("the fingerprint could not be made: {e}");
    }
    printer.finish();
    let compressed = FingerprintCompressor::from(&config).compress(printer.fingerprint());
    let duration_secs = walked.frames as f64 / f64::from(walked.rate);
    let sold = Sold::of(walked.codec, walked.bytes, duration_secs);
    Ok(Print {
        fingerprint: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(compressed),
        duration_secs,
        damaged: walked.damaged,
        flaws: flaws.map(|f| f.finish(sold)).unwrap_or_default(),
    })
}

/// One of AcoustID's answers: how sure, and the MusicBrainz recordings it names.
#[derive(Debug, Clone, Deserialize)]
pub struct Found {
    #[serde(default)]
    pub score: f32,
    #[serde(default)]
    pub recordings: Vec<Recording>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Recording {
    pub id: String,
}

#[derive(Deserialize)]
struct Answer {
    status: String,
    #[serde(default)]
    results: Vec<Found>,
    #[serde(default)]
    error: Option<Problem>,
}

#[derive(Deserialize)]
struct Problem {
    message: String,
}

/// What AcoustID knows of this fingerprint.
pub async fn lookup(key: &str, print: &Print) -> Result<Vec<Found>> {
    let http = reqwest::Client::builder().timeout(Duration::from_secs(30)).build()?;
    let duration = (print.duration_secs.round() as u64).to_string();
    let form = [("client", key), ("meta", "recordingids"), ("duration", duration.as_str()), ("fingerprint", print.fingerprint.as_str())];
    let answer: Answer = http
        .post("https://api.acoustid.org/v2/lookup")
        .form(&form)
        .send()
        .await
        .context("AcoustID could not be reached")?
        .json()
        .await
        .context("AcoustID's answer could not be read")?;
    if answer.status != "ok" {
        bail!("AcoustID: {}", answer.error.map_or(answer.status, |p| p.message));
    }
    Ok(answer.results)
}

/// The score at which AcoustID names `recording`, when it is sure enough.
pub fn names(found: &[Found], recording: &str) -> Option<f32> {
    found
        .iter()
        .filter(|f| f.score >= SURE && f.recordings.iter().any(|r| r.id.eq_ignore_ascii_case(recording)))
        .map(|f| f.score)
        .fold(None, |best, s| Some(best.map_or(s, |b: f32| b.max(s))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(score: f32, ids: &[&str]) -> Found {
        Found { score, recordings: ids.iter().map(|id| Recording { id: id.to_string() }).collect() }
    }

    #[test]
    fn the_recording_asked_for_passes_only_when_acoustid_is_sure() {
        let rec = "0b6f4bd4-2f0c-4b7e-9a51-6c1c0d8f7a11";
        assert_eq!(names(&[found(0.97, &["other", rec])], rec), Some(0.97));
        assert_eq!(names(&[found(0.85, &[rec])], rec), None, "below the gate");
        assert_eq!(names(&[found(0.99, &["other"])], rec), None, "another recording");
        assert_eq!(names(&[found(0.91, &[rec]), found(0.95, &[&rec.to_uppercase()])], rec), Some(0.95), "the surest");
        assert_eq!(names(&[], rec), None);
    }

    #[test]
    fn a_file_is_fingerprinted_in_chromaprints_own_form() {
        let dir = std::env::temp_dir().join(format!("cratarr-print-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tone.wav");
        // Ten seconds of a falling and rising tone, stereo, 44.1 kHz.
        let rate = 44_100u32;
        let samples: Vec<i16> = (0..rate * 10)
            .flat_map(|i| {
                let t = f64::from(i) / f64::from(rate);
                let v = ((t * (440.0 + 200.0 * (t * 0.7).sin()) * std::f64::consts::TAU).sin() * 12_000.0) as i16;
                [v, v]
            })
            .collect();
        let mut wav = Vec::new();
        let data = (samples.len() * 2) as u32;
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate * 4).to_le_bytes());
        wav.extend_from_slice(&4u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data.to_le_bytes());
        for s in &samples {
            wav.extend_from_slice(&s.to_le_bytes());
        }
        std::fs::write(&path, wav).unwrap();
        let p = print(&path).unwrap();
        assert!(p.fingerprint.starts_with("AQAA"), "algorithm 1 and a length, as fpcalc writes: {}", &p.fingerprint[..8.min(p.fingerprint.len())]);
        assert!((p.duration_secs - 10.0).abs() < 0.01, "{}", p.duration_secs);
        assert_eq!(p.damaged, 0);
        std::fs::remove_dir_all(&dir).ok();
    }
}
