//! Insights (decision 0011): figures about the library and the playlists, and how close the playlists are. Built by
//! the pure `crate::insights` from the catalogue and the playlist files, and kept a few minutes: it reads every track.
//! The playlists are the asking account's own (decision 0051). Collection's figures count the libraries switched on
//! (`without`, as the page's library switches keep them; the owner, 2026-10-23), the playlists the Music folder.

use std::sync::Arc;

use anyhow::Result;
use tokio::sync::Mutex;

use super::library::{Library, epoch_now};
use super::history::Period;
use crate::insights::Insights;

/// How long a built report is kept.
const KEEP_SECS: i64 = 300;
/// How many reports are kept (each account, each set of libraries): switching a library off and on again is instant.
const KEEP: usize = 4;

/// Whose report, from which server's libraries, without which of them, over which period.
type Key = (Option<String>, Option<String>, Vec<String>, Period);

pub struct InsightsService {
    lib: Arc<Library>,
    /// For the tracks in a crate.
    playlists: Arc<super::playlists::PlaylistService>,
    /// The last reports: when, and for what.
    kept: Mutex<Vec<(i64, Key, Arc<Insights>)>>,
}

impl InsightsService {
    /// The catalogue folders the libraries switched on keep (`without`, of the server `want`, else the first connected),
    /// for every part of Insights; `None` when none is switched off.
    pub fn folders_on(&self, want: Option<&str>, without: &[String]) -> Option<std::collections::HashSet<i64>> {
        if without.is_empty() {
            return None;
        }
        let connected = self.lib.connected();
        let server = want.and_then(|w| connected.iter().find(|(a, _)| a.cfg.id == w)).or_else(|| connected.first());
        let off: Vec<std::path::PathBuf> = match server {
            Some((a, _)) => super::library::folders_switched_off(a, without).into_iter().map(std::path::PathBuf::from).collect(),
            None => Vec::new(),
        };
        Some(self.lib.catalogue().folders().iter().enumerate().filter(|(_, f)| !off.contains(f)).map(|(i, _)| i as i64).collect())
    }

    pub(super) fn new(lib: Arc<Library>, playlists: Arc<super::playlists::PlaylistService>) -> Self {
        Self { lib, playlists, kept: Mutex::new(Vec::new()) }
    }

    /// The report, Collection's figures from the libraries not in `without` (`source/library` keys) of the server
    /// `want` (the one followed; else the first connected), as a crate draws from them, and of the albums that came in
    /// `period` (the map: whenever they came).
    pub async fn get(&self, want: Option<&str>, without: &[String], period: Period) -> Result<Arc<Insights>> {
        let mut off_keys = without.to_vec();
        off_keys.sort();
        let key: Key = (self.playlists.asking_account(), want.map(str::to_string), off_keys, period);
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
        let folders = catalogue.folders();
        let shown: Vec<i64> = match self.folders_on(want, without) {
            Some(on) => {
                let mut v: Vec<i64> = on.into_iter().collect();
                v.sort();
                v
            }
            None => (0..folders.len() as i64).collect(),
        };
        let incoming_paths = self.lib.folders().incoming;
        let incoming: std::collections::HashSet<i64> =
            folders.iter().enumerate().filter(|(_, f)| incoming_paths.iter().any(|p| std::path::Path::new(p) == f.as_path())).map(|(i, _)| i as i64).collect();
        // The overview's figures from Cratarr's own library and the asking account's plays and stars, where it is open.
        let served = self.playlists.served();
        // Whose plays: the request's account, read here (the blocking task below is not the request's).
        let user = served.as_ref().and_then(|s| s.current_user());
        let since = period.since(epoch_now());
        // The account's own crates' tracks (Listed); none where they cannot be worked out.
        let crated = self.playlists.crated_lines(want, without).await.unwrap_or_default();
        let report = tokio::task::spawn_blocking(move || -> Result<Insights> {
            // When each file's album came (Cratarr's own library), for the period.
            let arrivals = match &served {
                Some(s) => Some(s.library.album_arrivals()?),
                None => None,
            };
            let came: std::collections::HashMap<(i64, &str), i64> = match &arrivals {
                Some((albums, songs)) => {
                    let at: std::collections::HashMap<&str, i64> = albums.iter().filter_map(|(id, _, c)| c.map(|c| (id.as_str(), c))).collect();
                    songs.iter().filter_map(|(_, a, f, rel)| at.get(a.as_str()).map(|c| ((*f, rel.as_str()), *c))).collect()
                }
                None => Default::default(),
            };
            // An album with no known arrival is not in a period (nothing is made up); no library open, no period.
            let in_period = |folder: i64, rel: &str| match since {
                Some(s) if arrivals.is_some() => came.get(&(folder, rel)).is_some_and(|c| *c >= s),
                _ => true,
            };
            let rows = catalogue.track_rows(0)?;
            let (mut on_rows, mut shown_rows) = (Vec::new(), Vec::new());
            let mut in_crates = 0;
            for &folder in &shown {
                let folder_rows = if folder == 0 { rows.clone() } else { catalogue.track_rows(folder)? };
                for mut r in folder_rows {
                    let keep = in_period(folder, &r.rel);
                    // A crate names a track by its playlist line (an Incoming one's climbs out of Music).
                    if keep && !crated.is_empty() && catalogue.line_of(folder, &r.rel).is_some_and(|l| crated.contains(&l)) {
                        in_crates += 1;
                    }
                    // Another folder's albums are its own, whatever their path inside it.
                    if folder != 0 {
                        r.rel = format!("{folder}:{}", r.rel);
                    }
                    if keep {
                        shown_rows.push(r.clone());
                    }
                    on_rows.push(r);
                }
            }
            let mut report = crate::insights::build(&rows, &shown_rows, &on_rows, &rules, &playlists);
            report.playlists.library_tracks_in_crates = in_crates;
            if let (Some(served), Some((albums, songs))) = (served, &arrivals) {
                let (plays, starred): (std::collections::HashMap<String, u32>, std::collections::HashSet<String>) = match &user {
                    Some(u) => (
                        served.userdata.song_plays(u)?.into_iter().map(|(id, (n, _))| (id, n)).collect(),
                        served.userdata.starred(u, crate::userdata::Kind::Song)?.into_iter().collect(),
                    ),
                    None => Default::default(),
                };
                // Months in UTC: an album that came in the night's first hour may fall in the month before.
                // Growth's steps within the period, as taste over time's.
                use crate::insights::Step;
                let growth = match period {
                    Period::Week => (Step::Day, Some(7)),
                    Period::Month => (Step::Week, Some(5)),
                    Period::Year => (Step::Month, Some(12)),
                    Period::All => (Step::Year, None),
                };
                let mut overview = crate::insights::overview(albums, songs, &shown.iter().copied().collect(), &incoming, &plays, &starred, since, growth, epoch_now(), 0);
                // Most played by name, as the library has them.
                let ids: Vec<String> = overview.most_played.iter().map(|t| t.id.clone()).collect();
                let named: std::collections::HashMap<String, (String, String)> =
                    served.library.songs_by_id(&ids)?.into_iter().map(|s| (s.id, (s.title, s.artist))).collect();
                for t in &mut overview.most_played {
                    if let Some((title, artist)) = named.get(&t.id) {
                        t.title = title.clone();
                        t.artist = Some(artist.clone()).filter(|a| !a.is_empty());
                    }
                }
                overview.most_played.retain(|t| !t.title.is_empty());
                report.overview = Some(overview);
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
