//! Decision 0035: the NAS keeps the analysis up to date. The first pass over the whole library is the ThinkBook's
//! (`cratarr-server --analyse`, its results imported by file); after it, this pass analyses what comes or changes (a
//! new album, a replaced file, a newer analysis version), one file at a time, resting three times as long as each
//! took: about a quarter of one core, so Cratarr answers as fast as ever meanwhile.
//!
//! `CRATARR_ANALYSIS`: `keep` (the default: waits until most of the library has an analysis, as after the ThinkBook's
//! import, then keeps up), `all` (does the first pass itself, when no ThinkBook is at hand), `off`.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::analysis::Beats;
use crate::analysis::bands::Bands;
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
        // Files that could not be analysed, as they were: not tried again until Cratarr starts again.
        let mut failed: HashSet<(i64, String, u64, i64)> = HashSet::new();
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
                tokio::time::sleep(IDLE).await;
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

/// A file's sound in bands (decision 0046, amended): as kept, or worked out now, as `beats`.
pub async fn bands(cat: Arc<Catalogue>, folder: i64, rel: String) -> Option<Bands> {
    visual(cat, folder, rel, true).await.map(|(_, bands)| bands)
}

/// What is kept of a file's beats and bands, as the file is now. Bands of an older form count as not worked out.
fn kept(cat: &Catalogue, folder: i64, rel: &str) -> (Option<Beats>, Option<Bands>) {
    let bands = cat.bands_of(folder, rel).flatten().filter(|b| b.0.first().is_none_or(|f| *f == crate::analysis::bands::FORMAT));
    (cat.beats_of(folder, rel).flatten(), bands)
}

/// A file's beats and, when `with_bands`, its bands: as kept, or worked out now and kept. The page asks for both at
/// once for a new track: whichever comes first decodes the file once for all that is missing (beats, bands or both)
/// and keeps it, and the other finds it kept once it has its turn. What was kept stays as it was (byte for byte). A file
/// without an analysis of it as it is now gets the whole analysis kept, as the pass would. A decode that does not cover
/// the whole file gives no beats and no bands, kept as none, so it is not decoded again at every request.
async fn visual(cat: Arc<Catalogue>, folder: i64, rel: String, with_bands: bool) -> Option<(Beats, Bands)> {
    let enough = move |k: &(Option<Beats>, Option<Bands>)| k.0.is_some() && (!with_bands || k.1.is_some());
    let c = cat.clone();
    let r = rel.clone();
    let k = tokio::task::spawn_blocking(move || kept(&c, folder, &r)).await.ok()?;
    if enough(&k) {
        return Some((k.0.unwrap_or_default(), k.1.unwrap_or_default()));
    }
    // The permit is held here, not by the decode: a decode past its deadline goes on alone and lets the next one in.
    let _turn = tokio::time::timeout(TURN, ON_DEMAND.acquire()).await.ok()?.ok()?;
    let work = tokio::task::spawn_blocking(move || -> anyhow::Result<(Beats, Bands)> {
        // Kept while this one waited its turn.
        let (had_beats, had_bands) = kept(&cat, folder, &rel);
        if had_beats.is_some() && (!with_bands || had_bands.is_some()) {
            return Ok((had_beats.unwrap_or_default(), had_bands.unwrap_or_default()));
        }
        let (path, size, mtime) = cat.file_to_analyse(folder, &rel).ok_or_else(|| anyhow::anyhow!("not in the library"))?;
        let started = Instant::now();
        let (a, bands) = crate::analysis::analyse_with_bands(&path, &cat.genres_of_file(folder, &rel))?;
        #[cfg(test)]
        DECODED.lock().unwrap().push(path.clone());
        let whole = a.complete != Some(false);
        let beats = match had_beats {
            Some(b) => b,
            None => {
                let beats = if whole { a.beats.clone().unwrap_or_default() } else { Beats::default() };
                if cat.analysis_of(folder, &rel).is_none() {
                    cat.store_analysis(folder, &rel, size, mtime, &crate::analysis::Analysis { beats: Some(beats.clone()), ..a })?;
                } else {
                    cat.store_beats(folder, &rel, size, mtime, &beats)?;
                }
                beats
            }
        };
        let bands = match had_bands {
            Some(b) => b,
            None => {
                let bands = if whole { bands } else { Bands::default() };
                cat.store_bands(folder, &rel, size, mtime, &bands)?;
                bands
            }
        };
        tracing::info!("analysis: {rel}'s beats and bands when asked for, in {:.1} s", started.elapsed().as_secs_f64());
        Ok((beats, bands))
    });
    match tokio::time::timeout(DECODE, work).await {
        Ok(Ok(Ok(got))) => Some(got),
        Ok(Ok(Err(e))) => {
            tracing::warn!("analysis: beats and bands when asked for: {e:#}");
            None
        }
        Ok(Err(e)) => {
            tracing::warn!("analysis: beats and bands when asked for: {e}");
            None
        }
        Err(_) => {
            tracing::warn!("analysis: beats and bands when asked for took over {} s", DECODE.as_secs());
            None
        }
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

        // A new track, its beats and bands asked for at once: one decode, both kept.
        let (beats_one, bands_one) = tokio::join!(beats(cat.clone(), 0, "one.wav".into()), bands(cat.clone(), 0, "one.wav".into()));
        let (beats_one, bands_one) = (beats_one.unwrap(), bands_one.unwrap());
        assert_eq!(decodes() - before, 1, "one decode for both");
        assert!(beats_one.each().count() >= 55 && bands_one.read().is_some_and(|r| r.transients[2].len() >= 55));
        assert_eq!((cat.beats_of(0, "one.wav"), cat.bands_of(0, "one.wav")), (Some(Some(beats_one.clone())), Some(Some(bands_one.clone()))));
        // Asked again: kept, no decode.
        assert_eq!(bands(cat.clone(), 0, "one.wav".into()).await, Some(bands_one));
        assert_eq!(decodes() - before, 1);

        // A track whose beats are kept (here, other ones than its own would be) and its bands not yet: decoded for its
        // bands alone, and its beats stay byte for byte.
        let (path, size, mtime) = cat.file_to_analyse(0, "two.wav").unwrap();
        let kept = crate::analysis::Beats(vec![1, 2, 3, 4, 5]);
        let a = crate::analysis::analyse(&path, &[]).unwrap();
        cat.store_analysis(0, "two.wav", size, mtime, &crate::analysis::Analysis { beats: Some(kept.clone()), ..a }).unwrap();
        let got = bands(cat.clone(), 0, "two.wav".into()).await.unwrap();
        assert_eq!(decodes() - before, 2);
        assert!(got.read().is_some());
        assert_eq!(cat.beats_of(0, "two.wav"), Some(Some(kept.clone())));
        assert_eq!(beats(cat.clone(), 0, "two.wav".into()).await, Some(kept));
        assert_eq!(decodes() - before, 2, "beats kept: no decode");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn its_mode_comes_from_the_environment() {
        assert_eq!(Mode::from_env(None), Mode::Keep);
        assert_eq!(Mode::from_env(Some(" ALL ")), Mode::All);
        assert_eq!(Mode::from_env(Some("off")), Mode::Off);
        assert_eq!(Mode::from_env(Some("whatever")), Mode::Keep);
    }
}
