//! Decision 0035: the NAS keeps the analysis up to date. The first pass over the whole library is the ThinkBook's
//! (`cratarr-server --analyse`, its results imported by file); after it, this pass analyses what comes or changes (a
//! new album, a replaced file, a newer analysis version), one file at a time, resting three times as long as each
//! took: about a quarter of one core, so Cratarr answers as fast as ever meanwhile.
//!
//! When it has nothing to analyse, it hears new music's beats with Beat this! (decision 0049, amended; `beat_this`), a
//! file at a time, resting as long again; and with no beats left to hear either, it works out the waveform in three
//! bands (`analysis::band_wave`) of a file that has none yet, a file at a time, resting as long again.
//!
//! `CRATARR_ANALYSIS`: `keep` (the default: waits until most of the library has an analysis, as after the ThinkBook's
//! import, then keeps up), `all` (does the first pass itself, when no ThinkBook is at hand), `off`.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::analysis::Beats;
use crate::analysis::band_wave::BandWave;
use crate::analysis::signals::Signals;
use crate::catalogue::Catalogue;

/// Before the first look: Cratarr has just started, and its catalogue is still catching up.
const FIRST_LOOK: Duration = Duration::from_secs(120);
/// With nothing to do, looked at again after this long.
const IDLE: Duration = Duration::from_secs(600);
/// Rest after each file, as a multiple of what it took.
const REST: u32 = 3;
/// `keep` starts once this share (percent) of the library has an analysis.
const STARTED_AT: u64 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Keep,
    All,
    Off,
}

impl Mode {
    pub fn from_env(v: Option<&str>) -> Self {
        match v.map(|v| v.trim().to_lowercase()).as_deref() {
            Some("all") => Mode::All,
            Some("off") => Mode::Off,
            _ => Mode::Keep,
        }
    }
}

/// Runs the pass in the background for as long as Cratarr runs.
pub fn run(cat: Arc<Catalogue>, mode: Mode) {
    if mode == Mode::Off {
        tracing::info!("analysis: the NAS's pass is off (CRATARR_ANALYSIS=off)");
        return;
    }
    tokio::spawn(async move {
        tokio::time::sleep(FIRST_LOOK).await;
        // Files that could not be analysed, or their beats heard, as they were: not tried again until Cratarr starts
        // again.
        let mut failed: HashSet<(i64, String, u64, i64)> = HashSet::new();
        let mut unheard: HashSet<(i64, String, u64, i64)> = HashSet::new();
        let mut unbanded: HashSet<(i64, String, u64, i64)> = HashSet::new();
        let mut waiting_said = false;
        loop {
            let c = cat.clone();
            let counts = tokio::task::spawn_blocking(move || c.analysis_counts()).await.ok().and_then(|r| r.ok());
            if mode == Mode::Keep && counts.is_some_and(|(done, all)| all > 0 && done * 100 < all * STARTED_AT) {
                if !waiting_said {
                    let (done, all) = counts.unwrap_or_default();
                    tracing::info!("analysis: {done} of {all} files analysed; the NAS keeps up once the first pass is imported (or set CRATARR_ANALYSIS=all)");
                    waiting_said = true;
                }
                tokio::time::sleep(IDLE).await;
                continue;
            }
            let c = cat.clone();
            let batch = tokio::task::spawn_blocking(move || c.unanalysed(50)).await.ok().and_then(|r| r.ok()).unwrap_or_default();
            let batch: Vec<_> = batch.into_iter().filter(|(f, r, _, s, m)| !failed.contains(&(*f, r.clone(), *s, *m))).collect();
            if batch.is_empty() {
                // Nothing to analyse: new music's beats, one file, then a look again.
                let (c, limit) = (cat.clone(), 1 + unheard.len());
                let next = tokio::task::spawn_blocking(move || crate::app::beat_this::new_music(&c, limit))
                    .await
                    .ok()
                    .and_then(|r| r.ok())
                    .unwrap_or_default()
                    .into_iter()
                    .find(|(f, r, _, s, m)| !unheard.contains(&(*f, r.clone(), *s, *m)));
                let Some((folder, rel, path, size, mtime)) = next else {
                    // No beats to hear either: one file's waveform in three bands, else a rest.
                    let (c, limit) = (cat.clone(), 1 + unbanded.len());
                    let next = tokio::task::spawn_blocking(move || c.without_band_wave(limit))
                        .await
                        .ok()
                        .and_then(|r| r.ok())
                        .unwrap_or_default()
                        .into_iter()
                        .find(|(f, r, _, s, m)| !unbanded.contains(&(*f, r.clone(), *s, *m)));
                    let Some((folder, rel, path, size, mtime)) = next else {
                        tokio::time::sleep(IDLE).await;
                        continue;
                    };
                    let started = Instant::now();
                    let c = cat.clone();
                    let r = rel.clone();
                    let done = tokio::task::spawn_blocking(move || {
                        let _one = BAND_WAVES.lock().unwrap_or_else(|e| e.into_inner());
                        let w = crate::analysis::band_wave::work_out(&path)?;
                        c.store_band_wave(folder, &r, size, mtime, &w)
                    })
                    .await;
                    match done {
                        Ok(Ok(true)) => {}
                        Ok(Ok(false)) => {
                            unbanded.insert((folder, rel, size, mtime));
                        }
                        Ok(Err(e)) => {
                            tracing::warn!("band waveform: {rel}: {e:#}");
                            unbanded.insert((folder, rel, size, mtime));
                        }
                        Err(e) => {
                            tracing::warn!("band waveform: {rel}: {e}");
                            unbanded.insert((folder, rel, size, mtime));
                        }
                    }
                    tokio::time::sleep(started.elapsed() * REST).await;
                    continue;
                };
                let started = Instant::now();
                let c = cat.clone();
                let r = rel.clone();
                match tokio::task::spawn_blocking(move || crate::app::beat_this::hear_and_keep(&c, folder, &r, &path, size, mtime)).await {
                    Ok(Ok(n)) => tracing::info!("beats: {rel}: {n} heard in {:.1} s", started.elapsed().as_secs_f64()),
                    Ok(Err(e)) => {
                        tracing::warn!("beats: {rel}: {e:#}");
                        unheard.insert((folder, rel, size, mtime));
                    }
                    Err(e) => {
                        tracing::warn!("beats: {rel}: {e}");
                        unheard.insert((folder, rel, size, mtime));
                    }
                }
                tokio::time::sleep(started.elapsed() * REST).await;
                continue;
            }
            for (folder, rel, path, size, mtime) in batch {
                let started = Instant::now();
                let c = cat.clone();
                let r = rel.clone();
                let done = tokio::task::spawn_blocking(move || {
                    let genres = c.genres_of_file(folder, &r);
                    let a = crate::analysis::analyse(&path, &genres)?;
                    c.store_analysis(folder, &r, size, mtime, &a)
                })
                .await;
                match done {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        tracing::warn!("analysis: {rel}: {e:#}");
                        failed.insert((folder, rel, size, mtime));
                    }
                    Err(e) => {
                        tracing::warn!("analysis: {rel}: {e}");
                        failed.insert((folder, rel, size, mtime));
                    }
                }
                tokio::time::sleep(started.elapsed() * REST).await;
            }
        }
    });
}

/// One waveform in three bands worked out at a time, whether a play asked for it or the pass: a play's waits for the
/// pass's file to finish (a second or two).
static BAND_WAVES: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A file's waveform in three bands (`analysis::band_wave`): as kept, or worked out now and kept, the first time a
/// track plays. `None`: the file cannot be decoded, or it took too long (`DECODE`; it is still kept when it comes).
pub async fn band_wave(cat: Arc<Catalogue>, folder: i64, rel: String) -> Option<BandWave> {
    let (c, r) = (cat.clone(), rel.clone());
    let kept = tokio::task::spawn_blocking(move || c.band_wave_of(folder, &r)).await.ok().flatten();
    if let Some(Some(w)) = kept.filter(|k| k.as_ref().is_some_and(BandWave::current)) {
        return Some(w);
    }
    let work = tokio::task::spawn_blocking(move || -> anyhow::Result<BandWave> {
        let _one = BAND_WAVES.lock().unwrap_or_else(|e| e.into_inner());
        // Kept while this one waited.
        if let Some(Some(w)) = cat.band_wave_of(folder, &rel).filter(|k| k.as_ref().is_some_and(BandWave::current)) {
            return Ok(w);
        }
        let (path, size, mtime) = cat.file_to_analyse(folder, &rel).ok_or_else(|| anyhow::anyhow!("not in the library"))?;
        let w = crate::analysis::band_wave::work_out(&path)?;
        // Without an analysis of the file as it is now it is not kept: the pass analyses it first.
        cat.store_band_wave(folder, &rel, size, mtime, &w)?;
        Ok(w)
    });
    match tokio::time::timeout(DECODE, work).await {
        Ok(Ok(Ok(w))) => Some(w),
        Ok(Ok(Err(e))) => {
            tracing::warn!("band waveform when asked for: {e:#}");
            None
        }
        Ok(Err(e)) => {
            tracing::warn!("band waveform when asked for: {e}");
            None
        }
        Err(_) => {
            tracing::warn!("band waveform when asked for took over {} s", DECODE.as_secs());
            None
        }
    }
}

/// What the visuals ask for (decision 0046) worked out when first asked: one decode at a time, beside the pass.
static ON_DEMAND: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
/// The files decoded for the visuals, one entry a decode: the tests count their own files' (a count of all would count
/// another test's too, run beside them).
#[cfg(test)]
static DECODED: std::sync::Mutex<Vec<std::path::PathBuf>> = std::sync::Mutex::new(Vec::new());
/// How long a request waits for its turn, and then for its decode (a 40-minute track takes about 7 s on the NAS).
const TURN: Duration = Duration::from_secs(20);
const DECODE: Duration = Duration::from_secs(60);

/// A file's beats (decision 0046): as kept, or worked out now (see `visual`). `None`: the file cannot be analysed, or
/// its turn or its decode took too long (its result is still kept when it comes).
pub async fn beats(cat: Arc<Catalogue>, folder: i64, rel: String) -> Option<Beats> {
    visual(cat, folder, rel, false).await.map(|(beats, _)| beats)
}

/// A file's sound in signals (decision 0046, amended): as kept, or worked out now, as `beats`.
pub async fn signals(cat: Arc<Catalogue>, folder: i64, rel: String) -> Option<Signals> {
    visual(cat, folder, rel, true).await.map(|(_, signals)| signals)
}

/// What is kept of a file's beats and signals, as the file is now. Signals of an older form count as not worked out;
/// so do both while Beat this! has heard the file and the beats kept are not its (decision 0049, amended).
fn kept(cat: &Catalogue, folder: i64, rel: &str) -> (Option<Beats>, Option<Signals>) {
    if !cat.beats_are_beat_this(folder, rel) && cat.beat_this_of(folder, rel).is_some() {
        return (None, None);
    }
    let signals = cat.signals_of(folder, rel).flatten().filter(|b| b.0.first().is_none_or(|f| *f == crate::analysis::signals::FORMAT));
    (cat.beats_of(folder, rel).flatten(), signals)
}

/// A file's beats and, when `with_signals`, its signals: as kept, or worked out now and kept. The page asks for both at
/// once for a new track: whichever comes first decodes the file once for all that is missing (beats, signals or both)
/// and keeps it, and the other finds it kept once it has its turn. What was kept stays as it was (byte for byte), until
/// Beat this! has heard the file: then its beats and downbeats replace the analysis' own, and the signals follow them. A file
/// without an analysis of it as it is now gets the whole analysis kept, as the pass would. A decode that does not cover
/// the whole file gives no beats and no signals, kept as none, so it is not decoded again at every request.
async fn visual(cat: Arc<Catalogue>, folder: i64, rel: String, with_signals: bool) -> Option<(Beats, Signals)> {
    let enough = move |k: &(Option<Beats>, Option<Signals>)| k.0.is_some() && (!with_signals || k.1.is_some());
    let c = cat.clone();
    let r = rel.clone();
    let k = tokio::task::spawn_blocking(move || kept(&c, folder, &r)).await.ok()?;
    if enough(&k) {
        return Some((k.0.unwrap_or_default(), k.1.unwrap_or_default()));
    }
    // The permit is held here, not by the decode: a decode past its deadline goes on alone and lets the next one in.
    let _turn = tokio::time::timeout(TURN, ON_DEMAND.acquire()).await.ok()?.ok()?;
    let work = tokio::task::spawn_blocking(move || work_out(&cat, folder, &rel, with_signals));
    match tokio::time::timeout(DECODE, work).await {
        Ok(Ok(Ok(got))) => Some(got),
        Ok(Ok(Err(e))) => {
            tracing::warn!("analysis: beats and signals when asked for: {e:#}");
            None
        }
        Ok(Err(e)) => {
            tracing::warn!("analysis: beats and signals when asked for: {e}");
            None
        }
        Err(_) => {
            tracing::warn!("analysis: beats and signals when asked for took over {} s", DECODE.as_secs());
            None
        }
    }
}

/// The decode for the visuals, blocking: whatever of the beats and signals is missing, worked out and kept (see `visual`).
fn work_out(cat: &Catalogue, folder: i64, rel: &str, with_signals: bool) -> anyhow::Result<(Beats, Signals)> {
    // Kept while this one waited its turn.
    let (had_beats, had_signals) = kept(&cat, folder, &rel);
    if had_beats.is_some() && (!with_signals || had_signals.is_some()) {
        return Ok((had_beats.unwrap_or_default(), had_signals.unwrap_or_default()));
    }
    let (path, size, mtime) = cat.file_to_analyse(folder, &rel).ok_or_else(|| anyhow::anyhow!("not in the library"))?;
    let started = Instant::now();
    let heard = cat.beat_this_of(folder, rel).map(|(beats, downbeats)| crate::analysis::Heard { beats, downbeats });
    let (a, signals) = crate::analysis::analyse_with_signals(&path, &cat.genres_of_file(folder, &rel), heard.as_ref())?;
    #[cfg(test)]
    DECODED.lock().unwrap().push(path.clone());
    let whole = a.complete != Some(false);
    let beats = match had_beats {
        Some(b) => b,
        None => {
            let beats = if whole { a.beats.clone().unwrap_or_default() } else { Beats::default() };
            if cat.analysis_of(folder, &rel).is_none() {
                cat.store_analysis(folder, &rel, size, mtime, &crate::analysis::Analysis { beats: Some(beats.clone()), ..a })?;
            }
            cat.store_beats(folder, &rel, size, mtime, &beats, heard.is_some())?;
            beats
        }
    };
    let signals = match had_signals {
        Some(b) => b,
        None => {
            let signals = if whole { signals } else { Signals::default() };
            cat.store_signals(folder, &rel, size, mtime, &signals)?;
            signals
        }
    };
    tracing::info!("analysis: {rel}'s beats and signals when asked for, in {:.1} s", started.elapsed().as_secs_f64());
    Ok((beats, signals))
}

/// Tracks ahead in a player's queue whose signals are worked out while the one before plays.
const AHEAD: usize = 2;

/// Look-ahead (decision 0046, amended): the next tracks in each playing or paused player's queue get their signals
/// worked out before they play, so the visuals have them at once. On every change of a player (a queue being built
/// settles first) and once a minute; only on a free turn, as a page waiting for a decode comes first.
pub fn look_ahead(cat: Arc<Catalogue>, play_on: Arc<crate::app::playon::PlayOn>) {
    tokio::spawn(async move {
        let mut changes = play_on.subscribe();
        loop {
            if let Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) = tokio::time::timeout(Duration::from_secs(60), changes.recv()).await {
                return;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
            while changes.try_recv().is_ok() {}
            let queues = play_on
                .list()
                .into_iter()
                .filter(|p| p.playing || p.state == "paused")
                .filter_map(|p| play_on.get(&p.name))
                .map(|v| (v.index, v.queue.unwrap_or_default().into_iter().map(|t| (t.source, t.id)).collect::<Vec<_>>()))
                .collect::<Vec<_>>();
            for (source, id) in upcoming(&queues) {
                let c = cat.clone();
                let Ok(Some((folder, rel))) = tokio::task::spawn_blocking(move || c.track_of_source(&source, &id).and_then(|t| c.file_of(t))).await else {
                    continue;
                };
                ahead(cat.clone(), folder, rel).await;
            }
        }
    });
}

/// The next `AHEAD` tracks of each queue (its playing place, its tracks as source and id), each once.
fn upcoming(queues: &[(i64, Vec<(String, String)>)]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (index, tracks) in queues {
        let from = usize::try_from(index + 1).unwrap_or(0);
        for t in tracks.iter().skip(from).take(AHEAD) {
            if !out.contains(t) {
                out.push(t.clone());
            }
        }
    }
    out
}

/// One track's beats and signals ahead of its turn, when they are not kept yet and no page is waiting for a decode.
async fn ahead(cat: Arc<Catalogue>, folder: i64, rel: String) {
    let (c, r) = (cat.clone(), rel.clone());
    let Ok((beats, signals)) = tokio::task::spawn_blocking(move || kept(&c, folder, &r)).await else { return };
    if beats.is_some() && signals.is_some() {
        return;
    }
    let Ok(_turn) = ON_DEMAND.try_acquire() else { return };
    let r = rel.clone();
    match tokio::task::spawn_blocking(move || work_out(&cat, folder, &rel, true)).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => tracing::warn!("analysis: {r}'s signals ahead of its turn: {e:#}"),
        Err(e) => tracing::warn!("analysis: {r}'s signals ahead of its turn: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_visuals_decode_a_track_once_and_what_was_kept_stays_as_it_was() {
        let d = std::env::temp_dir().join(format!("cratarr-visuals-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let clicks: Vec<f64> = (0..60).map(|k| 0.25 + 0.5 * k as f64).collect();
        for name in ["one", "two"] {
            let made = crate::analysis::tests::click_track(&format!("visuals-{name}"), 30.0, &clicks, |_| 1.0);
            std::fs::rename(&made, d.join(format!("{name}.wav"))).unwrap();
        }
        let cat = Arc::new(Catalogue::open(None, vec![d.clone()]).unwrap());
        cat.walk_all();
        let decodes = || DECODED.lock().unwrap().iter().filter(|p| p.starts_with(&d)).count();
        let before = decodes();

        // A new track, its beats and signals asked for at once: one decode, both kept.
        let (beats_one, signals_one) = tokio::join!(beats(cat.clone(), 0, "one.wav".into()), signals(cat.clone(), 0, "one.wav".into()));
        let (beats_one, signals_one) = (beats_one.unwrap(), signals_one.unwrap());
        assert_eq!(decodes() - before, 1, "one decode for both");
        assert!(beats_one.each().count() >= 55 && signals_one.hits(crate::analysis::signals::id::HITS).is_some_and(|h| h.iter().any(|band| band.len() >= 55)));
        assert_eq!((cat.beats_of(0, "one.wav"), cat.signals_of(0, "one.wav")), (Some(Some(beats_one.clone())), Some(Some(signals_one.clone()))));
        // Asked again: kept, no decode.
        assert_eq!(signals(cat.clone(), 0, "one.wav".into()).await, Some(signals_one));
        assert_eq!(decodes() - before, 1);

        // A track whose beats are kept (here, other ones than its own would be) and its signals not yet: decoded for its
        // signals alone, and its beats stay byte for byte.
        let (path, size, mtime) = cat.file_to_analyse(0, "two.wav").unwrap();
        let kept = crate::analysis::Beats(vec![1, 2, 3, 4, 5]);
        let a = crate::analysis::analyse(&path, &[]).unwrap();
        cat.store_analysis(0, "two.wav", size, mtime, &crate::analysis::Analysis { beats: Some(kept.clone()), ..a }).unwrap();
        let got = signals(cat.clone(), 0, "two.wav".into()).await.unwrap();
        assert_eq!(decodes() - before, 2);
        assert!(got.read().is_some());
        assert_eq!(cat.beats_of(0, "two.wav"), Some(Some(kept.clone())));
        assert_eq!(beats(cat.clone(), 0, "two.wav".into()).await, Some(kept));
        assert_eq!(decodes() - before, 2, "beats kept: no decode");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_next_tracks_of_each_queue_are_looked_ahead_to_once() {
        let t = |id: &str| ("cratarr".to_string(), id.to_string());
        let queues = vec![
            (0, vec![t("a"), t("b"), t("c"), t("d")]),
            // Nothing playing yet: its first two.
            (-1, vec![t("c"), t("e"), t("f")]),
            // At its end: none.
            (1, vec![t("x"), t("y")]),
        ];
        assert_eq!(upcoming(&queues), vec![t("b"), t("c"), t("e")]);
    }

    #[test]
    fn its_mode_comes_from_the_environment() {
        assert_eq!(Mode::from_env(None), Mode::Keep);
        assert_eq!(Mode::from_env(Some(" ALL ")), Mode::All);
        assert_eq!(Mode::from_env(Some("off")), Mode::Off);
        assert_eq!(Mode::from_env(Some("whatever")), Mode::Keep);
    }
}
