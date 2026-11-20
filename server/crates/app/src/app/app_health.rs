//! Health as the app answers it: its checks are Health's (`health::Checks`), on the app as their host.

use super::App;
use super::health::{Checks, RecordHealth, Report};

impl App {
    /// Every domain's findings now (decision 0063), worst first, the most recent first among equals.
    pub async fn health_report(&self) -> Report {
        Checks(self).health_report().await
    }

    /// One record's Health (`Checks::record_health`).
    pub async fn record_health(&self, source: &str, album: &str) -> anyhow::Result<RecordHealth> {
        Checks(self).record_health(source, album).await
    }
}
