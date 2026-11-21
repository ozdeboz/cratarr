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
use crate::app::turn::Turn;

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

/// AcoustID's own error answer (`status: error`): its code and its words (TN.1). Reads as "AcoustID: " and its words.
#[derive(Debug, Deserialize)]
pub struct Problem {
    #[serde(default)]
    pub code: u32,
    #[serde(default)]
    message: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AcoustID: {}", self.message)
    }
}

impl std::error::Error for Problem {}

/// AcoustID's codes the failure class reads (its web service's list): a fingerprint it refused, a duration it refused, its rate limit.
pub const BAD_PRINT: u32 = 3;
pub const BAD_DURATION: u32 = 8;
pub const LIMIT: u32 = 14;

/// AcoustID's one turn (O6, TN): its limit is three questions a second, whoever asks (the gate, every account's hearing); its limit
/// answer holds them all.
pub(crate) static TURN: Turn = Turn::new(crate::app::turn::ACOUSTID);
/// Where AcoustID's lookup is asked.
const LOOKUP: &str = "https://api.acoustid.org/v2/lookup";

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    // The test's fake AcoustID on this thread (`FakeAcoustid`): its lookup's address, and a turn of its own, so its limit holds no
    // other test.
    static FAKE: std::cell::RefCell<Option<(Fake, std::sync::Arc<Turn>)>> = const { std::cell::RefCell::new(None) };
}

/// Where a test's fake AcoustID answers: at an address (plain http), or in the process, each form answered as it comes.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone)]
enum Fake {
    At(String),
    Answering(std::sync::Arc<dyn Fn(Vec<(&'static str, String)>) -> Heard + Send + Sync>),
}

/// Tests: this thread's lookups go to a fake AcoustID at `at` (its lookup's address, plain http) in a turn of its own, until this
/// is dropped (as `discover::FakeMusicBrainz` is MusicBrainz's).
#[cfg(any(test, feature = "test-support"))]
pub struct FakeAcoustid;

#[cfg(any(test, feature = "test-support"))]
impl FakeAcoustid {
    pub fn at(at: String) -> Self {
        FAKE.with(|f| *f.borrow_mut() = Some((Fake::At(at), std::sync::Arc::new(Turn::new(crate::app::turn::ACOUSTID)))));
        FakeAcoustid
    }

    /// A fake answering in the process (no socket, so a test on tokio's paused clock keeps its time): each lookup's form answered
    /// by `answer`, in a turn of its own.
    pub fn answering(answer: impl Fn(Vec<(&'static str, String)>) -> Heard + Send + Sync + 'static) -> Self {
        FAKE.with(|f| *f.borrow_mut() = Some((Fake::Answering(std::sync::Arc::new(answer)), std::sync::Arc::new(Turn::new(crate::app::turn::ACOUSTID)))));
        FakeAcoustid
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Drop for FakeAcoustid {
    fn drop(&mut self) {
        FAKE.with(|f| *f.borrow_mut() = None);
    }
}

/// What AcoustID knows of this fingerprint: asked in its turn, with Cratarr's User-Agent, its answer noted for Health (O6). Its limit
/// answer is the pause it asks for (TN.1), its error answer a `Problem`.
pub async fn lookup(key: &str, print: &Print) -> Result<Vec<Found>> {
    #[cfg(any(test, feature = "test-support"))]
    if let Some((fake, turn)) = FAKE.with(|f| f.borrow().clone()) {
        return match fake {
            Fake::At(at) => {
                let http = reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).user_agent(crate::app::reach::USER_AGENT).build()?;
                lookup_in(&turn, |form| posted(http, at, form), key, print).await
            }
            Fake::Answering(answer) => lookup_in(&turn, |form| std::future::ready(Ok(answer(form))), key, print).await,
        };
    }
    let http = crate::app::reach::client(crate::app::reach::Purpose::Metadata).timeout(Duration::from_secs(30)).build()?;
    lookup_in(&TURN, |form| posted(http, LOOKUP.to_string(), form), key, print).await
}

/// AcoustID's answer as it came: its status, its headers and its body.
pub struct Heard {
    pub status: reqwest::StatusCode,
    pub headers: reqwest::header::HeaderMap,
    pub body: String,
}

/// A lookup's form posted to `at` through `http`.
pub async fn posted(http: reqwest::Client, at: String, form: Vec<(&'static str, String)>) -> Result<Heard> {
    use crate::app::reach::Sent as _;
    let r = http.post(at).form(&form).sent().await.context("AcoustID could not be reached")?;
    let (status, headers) = (r.status(), r.headers().clone());
    let body = r.text().await.context("AcoustID's answer could not be read")?;
    Ok(Heard { status, headers, body })
}

/// `lookup`, in `turn`, its form sent by `send` (the tests' stand in for AcoustID). Its limit (a 429, or its code 14 whatever the
/// status) holds the turn for its Retry-After or its default (TN.2), and is the pause; any other answer ends a run.
pub async fn lookup_in<S, F>(turn: &Turn, send: S, key: &str, print: &Print) -> Result<Vec<Found>>
where
    S: FnOnce(Vec<(&'static str, String)>) -> F,
    F: std::future::Future<Output = Result<Heard>>,
{
    turn.turn().await;
    let duration = (print.duration_secs.round() as u64).to_string();
    let form = vec![("client", key.to_string()), ("meta", "recordingids".to_string()), ("duration", duration), ("fingerprint", print.fingerprint.clone())];
    let heard = send(form).await?;
    let answer: Option<Answer> = serde_json::from_str(&heard.body).ok();
    let code = answer.as_ref().and_then(|a| a.error.as_ref()).map(|p| p.code);
    if heard.status == reqwest::StatusCode::TOO_MANY_REQUESTS || code == Some(LIMIT) {
        return Err(turn.limited(crate::app::turn::retry_after(&heard.headers, crate::app::moment::now_secs())).into());
    }
    turn.heard(false, None);
    let Some(answer) = answer else {
        if !heard.status.is_success() {
            return Err(crate::app::turn::Refused::new("acoustid", heard.status, format!("AcoustID answered {}", heard.status)).into());
        }
        bail!("AcoustID's answer could not be read");
    };
    if answer.status != "ok" {
        return Err(answer.error.unwrap_or(Problem { code: 0, message: answer.status }).into());
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

    fn a_print() -> Print {
        Print { fingerprint: "AQAAT0mUaEkSRZEGAA".into(), duration_secs: 241.0, damaged: 0, flaws: Default::default() }
    }

    /// TN.1: AcoustID over its limit (its code 14, whatever the HTTP status; the body a stand-in until a real one is seen) is a pause
    /// until its Retry-After, else its default, and holds its turn: never AcoustID away, nor the file's. Its key goes in the form
    /// alone, and only the lookup is asked.
    #[tokio::test]
    async fn acoustid_over_its_limit_is_a_pause() {
        use crate::app::turn::{Failure, Pinned, class, fakes};
        let _one = crate::app::switches::TEST_STATE.write().await;
        let pinned = Pinned::now();
        let t0 = pinned.0;
        let limit = r#"{"status":"error","error":{"code":14,"message":"rate limit exceeded"}}"#;
        for (status, said, until) in [(429, None, t0 + 60), (200, None, t0 + 60), (429, Some("120"), t0 + 120), (400, Some("30"), t0 + 30)] {
            let violations = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
            let wrong = violations.clone();
            let code = axum::http::StatusCode::from_u16(status).unwrap();
            let answer = move |uri: axum::http::Uri, body: String| {
                let form: Vec<(String, String)> = serde_urlencoded::from_str(&body).unwrap_or_default();
                let field = |k: &str| form.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
                if uri.path() != "/v2/lookup" || uri.query().is_some() || field("client").as_deref() != Some("key") || field("meta").as_deref() != Some("recordingids") {
                    wrong.lock().unwrap().push(format!("asked {uri} with {body}"));
                }
                let mut r = axum::response::IntoResponse::into_response((code, limit));
                if let Some(said) = said {
                    r.headers_mut().insert(axum::http::header::RETRY_AFTER, said.parse().unwrap());
                }
                std::future::ready(r)
            };
            let _fake = FakeAcoustid::at(format!("{}/v2/lookup", fakes::serve(axum::Router::new().fallback(answer)).await));
            let e = lookup("key", &a_print()).await.unwrap_err();
            assert_eq!(class(&e), Failure::Paused { until }, "{status} {said:?}: {e:#}");
            assert!(!e.is::<Problem>(), "never its own error answer");
            let held = FAKE.with(|f| f.borrow().as_ref().map(|(_, turn)| turn.paused_until()));
            assert_eq!(held, Some(Some(until)), "its turn held until then");
            assert!(violations.lock().unwrap().is_empty(), "{:?}", violations.lock().unwrap());
        }
    }

    /// O6 (a pin): AcoustID's turn spaces every asker's lookups a third of a second apart, the gate's and a card's alike.
    #[tokio::test(start_paused = true)]
    async fn acoustid_is_asked_a_third_of_a_second_apart() {
        let turn = Turn::new(crate::app::turn::ACOUSTID);
        let at = std::sync::Mutex::new(Vec::new());
        let print = a_print();
        let send = |_: Vec<(&'static str, String)>| {
            at.lock().unwrap().push(tokio::time::Instant::now());
            let ok = Heard { status: reqwest::StatusCode::OK, headers: Default::default(), body: r#"{"status":"ok","results":[]}"#.into() };
            std::future::ready(Ok(ok))
        };
        let ask = || lookup_in(&turn, send, "key", &print);
        let (a, b, c, d, e) = tokio::join!(ask(), ask(), ask(), ask(), ask());
        assert!([a, b, c, d, e].iter().all(Result::is_ok));
        let mut at = at.into_inner().unwrap();
        at.sort();
        assert_eq!(at.len(), 5);
        assert!(at.windows(2).all(|w| w[1] - w[0] >= Duration::from_millis(350)), "{at:?}");
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
