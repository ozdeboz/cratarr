//! Insights (decision 0011): figures about the library and the playlists, and how close the playlists are. Built by
//! the pure `crate::insights` from the catalogue and the playlist files, and kept a few minutes: it reads every track.

use std::sync::Arc;

use anyhow::Result;
use tokio::sync::Mutex;

use super::library::{Library, epoch_now};
use crate::insights::Insights;

/// How long a built report is kept.
const KEEP_SECS: i64 = 300;

pub struct InsightsService {
    lib: Arc<Library>,
    kept: Mutex<Option<(i64, Arc<Insights>)>>,
}

impl InsightsService {
    pub(super) fn new(lib: Arc<Library>) -> Self {
        Self { lib, kept: Mutex::new(None) }
    }

    pub async fn get(&self) -> Result<Arc<Insights>> {
        let mut kept = self.kept.lock().await;
        if let Some((at, report)) = kept.as_ref()
            && epoch_now() - at < KEEP_SECS
        {
            return Ok(report.clone());
        }
        let playlists = match self.lib.store_opt() {
            Some(store) => store.snapshot().await?,
            None => Vec::new(),
        };
        let (catalogue, rules) = (self.lib.catalogue(), self.lib.tag_rules());
        let report = tokio::task::spawn_blocking(move || -> Result<Insights> {
            let rows = catalogue.track_rows(0)?;
            Ok(crate::insights::build(&rows, &rules, &playlists))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        let report = Arc::new(report);
        *kept = Some((epoch_now(), report.clone()));
        Ok(report)
    }
}
