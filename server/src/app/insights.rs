//! Insights (decision 0011): figures about the library and the playlists, and how close the playlists are. Built by
//! the pure `crate::insights` from the catalogue and the playlist files, and kept a few minutes: it reads every track.
//! The playlists are the asking account's own (decision 0051). Collection's figures count the libraries switched on
//! (`without`, as the page's library switches keep them; the owner, 2026-10-23), the playlists the Music folder.

use std::sync::Arc;

use anyhow::Result;
use tokio::sync::Mutex;

use super::library::{Library, epoch_now};
use crate::insights::Insights;

/// How long a built report is kept.
const KEEP_SECS: i64 = 300;
/// How many reports are kept (each account, each set of libraries): switching a library off and on again is instant.
const KEEP: usize = 4;

/// Whose report, from which server's libraries, and without which of them.
type Key = (Option<String>, Option<String>, Vec<String>);

pub struct InsightsService {
    lib: Arc<Library>,
    /// The last reports: when, and for what.
    kept: Mutex<Vec<(i64, Key, Arc<Insights>)>>,
}

impl InsightsService {
    pub(super) fn new(lib: Arc<Library>) -> Self {
        Self { lib, kept: Mutex::new(Vec::new()) }
    }

    /// The report, Collection's figures from the libraries not in `without` (`source/library` keys) of the server
    /// `want` (the one followed; else the first connected), as a crate draws from them.
    pub async fn get(&self, want: Option<&str>, without: &[String]) -> Result<Arc<Insights>> {
        let mut off_keys = without.to_vec();
        off_keys.sort();
        let key: Key = (super::playlists::asking_account(), want.map(str::to_string), off_keys);
        let mut kept = self.kept.lock().await;
        kept.retain(|(at, _, _)| epoch_now() - at < KEEP_SECS);
        if let Some((_, _, report)) = kept.iter().find(|(_, k, _)| *k == key) {
            return Ok(report.clone());
        }
        let playlists = match self.lib.store_for(key.0.as_deref()) {
            Ok(store) => store.snapshot().await?,
            Err(_) => Vec::new(),
        };
        let (catalogue, rules) = (self.lib.catalogue(), self.lib.tag_rules());
        let connected = self.lib.connected();
        let server = want.and_then(|w| connected.iter().find(|(a, _)| a.cfg.id == w)).or_else(|| connected.first());
        let off: Vec<std::path::PathBuf> = match server {
            Some((a, _)) if !without.is_empty() => super::library::folders_switched_off(a, without).into_iter().map(std::path::PathBuf::from).collect(),
            _ => Vec::new(),
        };
        let folders = catalogue.folders();
        let shown: Vec<i64> = folders.iter().enumerate().filter(|(_, f)| !off.contains(f)).map(|(i, _)| i as i64).collect();
        let incoming_paths = self.lib.folders().incoming;
        let incoming: std::collections::HashSet<i64> =
            folders.iter().enumerate().filter(|(_, f)| incoming_paths.iter().any(|p| std::path::Path::new(p) == f.as_path())).map(|(i, _)| i as i64).collect();
        // The overview's figures from Cratarr's own library and the asking account's plays and stars, where it is open.
        let served = crate::serve::instance();
        // Whose plays: the request's account, read here (the blocking task below is not the request's).
        let user = served.as_ref().and_then(|s| s.current_user());
        let report = tokio::task::spawn_blocking(move || -> Result<Insights> {
            let rows = catalogue.track_rows(0)?;
            let mut shown_rows = Vec::new();
            for &folder in &shown {
                if folder == 0 {
                    shown_rows.extend(rows.iter().cloned());
                    continue;
                }
                // Another folder's albums are its own, whatever their path inside it.
                shown_rows.extend(catalogue.track_rows(folder)?.into_iter().map(|mut r| {
                    r.rel = format!("{folder}:{}", r.rel);
                    r
                }));
            }
            let mut report = crate::insights::build(&rows, &shown_rows, &rules, &playlists);
            if let Some(served) = served {
                let (played, starred) = match &user {
                    Some(u) => (served.userdata.played(u)?, served.userdata.starred(u, crate::userdata::Kind::Song)?.into_iter().collect()),
                    None => Default::default(),
                };
                let (albums, songs) = served.library.album_arrivals()?;
                // Months in UTC: an album that came in the night's first hour may fall in the month before.
                report.overview = Some(crate::insights::overview(&albums, &songs, &shown.iter().copied().collect(), &incoming, &played, &starred, epoch_now(), 0));
            }
            Ok(report)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        let report = Arc::new(report);
        if kept.len() >= KEEP {
            kept.remove(0);
        }
        kept.push((epoch_now(), key, report.clone()));
        Ok(report)
    }
}
