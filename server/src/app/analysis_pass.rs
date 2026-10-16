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
