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

/// Beats worked out when they are asked for (decision 0046): one decode at a time, beside the pass.
static ON_DEMAND: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
/// How long a request waits for its turn, and then for its decode (a 40-minute track takes about 7 s on the NAS).
const TURN: Duration = Duration::from_secs(20);
const DECODE: Duration = Duration::from_secs(60);

/// A file's beats (decision 0046): as kept, or worked out now and kept: from an analysis of the file as it is now (a
/// file without one gets the whole analysis kept, as the pass would). A decode that does not cover the whole file
/// gives no beats, kept as none, so it is not decoded again at every request. `None`: the file cannot be analysed, or
/// its turn or its decode took too long (its result is still kept when it comes).
pub async fn beats(cat: Arc<Catalogue>, folder: i64, rel: String) -> Option<crate::analysis::Beats> {
    let c = cat.clone();
    let r = rel.clone();
    if let Some(Some(b)) = tokio::task::spawn_blocking(move || c.beats_of(folder, &r)).await.ok()? {
        return Some(b);
    }
    // The permit is held here, not by the decode: a decode past its deadline goes on alone and lets the next one in.
    let _turn = tokio::time::timeout(TURN, ON_DEMAND.acquire()).await.ok()?.ok()?;
    let work = tokio::task::spawn_blocking(move || -> anyhow::Result<crate::analysis::Beats> {
        // Kept while this one waited its turn.
        if let Some(Some(b)) = cat.beats_of(folder, &rel) {
            return Ok(b);
        }
        let (path, size, mtime) = cat.file_to_analyse(folder, &rel).ok_or_else(|| anyhow::anyhow!("not in the library"))?;
        let started = Instant::now();
        let a = crate::analysis::analyse(&path, &cat.genres_of_file(folder, &rel))?;
        let beats = if a.complete == Some(false) { crate::analysis::Beats::default() } else { a.beats.clone().unwrap_or_default() };
        if cat.analysis_of(folder, &rel).is_none() {
            cat.store_analysis(folder, &rel, size, mtime, &crate::analysis::Analysis { beats: Some(beats.clone()), ..a })?;
        } else {
            cat.store_beats(folder, &rel, size, mtime, &beats)?;
        }
        tracing::info!("analysis: {rel}'s beats when asked for, {} of them, in {:.1} s", beats.each().count(), started.elapsed().as_secs_f64());
        Ok(beats)
    });
    match tokio::time::timeout(DECODE, work).await {
        Ok(Ok(Ok(b))) => Some(b),
        Ok(Ok(Err(e))) => {
            tracing::warn!("analysis: beats when asked for: {e:#}");
            None
        }
        Ok(Err(e)) => {
            tracing::warn!("analysis: beats when asked for: {e}");
            None
        }
        Err(_) => {
            tracing::warn!("analysis: beats when asked for took over {} s", DECODE.as_secs());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_mode_comes_from_the_environment() {
        assert_eq!(Mode::from_env(None), Mode::Keep);
        assert_eq!(Mode::from_env(Some(" ALL ")), Mode::All);
        assert_eq!(Mode::from_env(Some("off")), Mode::Off);
        assert_eq!(Mode::from_env(Some("whatever")), Mode::Keep);
    }
}
