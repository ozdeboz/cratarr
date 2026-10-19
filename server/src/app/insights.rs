//! Insights (decision 0011): figures about the library and the playlists, and how close the playlists are. Built by
//! the pure `crate::insights` from the catalogue and the playlist files, and kept a few minutes: it reads every track.
//! The playlists are the asking account's own (decision 0051).

use std::sync::Arc;

use anyhow::Result;
use tokio::sync::Mutex;

use super::library::{Library, epoch_now};
use crate::insights::Insights;

/// How long a built report is kept.
const KEEP_SECS: i64 = 300;

pub struct InsightsService {
    lib: Arc<Library>,
    /// The last report: when, and whose playlists it counted.
    kept: Mutex<Option<(i64, Option<String>, Arc<Insights>)>>,
}

impl InsightsService {
    pub(super) fn new(lib: Arc<Library>) -> Self {
        Self { lib, kept: Mutex::new(None) }
    }

    pub async fn get(&self) -> Result<Arc<Insights>> {
        let account = super::playlists::asking_account();
        let mut kept = self.kept.lock().await;
        if let Some((at, whose, report)) = kept.as_ref()
            && *whose == account
            && epoch_now() - at < KEEP_SECS
        {
            return Ok(report.clone());
        }
        let playlists = match self.lib.store_for(account.as_deref()) {
            Ok(store) => store.snapshot().await?,
            Err(_) => Vec::new(),
        };
        let (catalogue, rules) = (self.lib.catalogue(), self.lib.tag_rules());
        let report = tokio::task::spawn_blocking(move || -> Result<Insights> {
            let rows = catalogue.track_rows(0)?;
            Ok(crate::insights::build(&rows, &rules, &playlists))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??;
        let report = Arc::new(report);
        *kept = Some((epoch_now(), account, report.clone()));
        Ok(report)
    }
}
