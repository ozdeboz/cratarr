//! Health as the app answers it: its checks are Health's (`health::Checks`), on the app as their host (`health_host::HealthHost`).

use std::sync::Arc;

use super::App;
use super::health_host::{Asked, HealthHost};
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

impl HealthHost for App {
    fn served(&self) -> &Option<Arc<crate::serve::Serve>> {
        &self.served
    }

    fn library(&self) -> &Arc<super::library::Library> {
        &self.library
    }

    fn playlists(&self) -> &Arc<super::playlists::PlaylistService> {
        &self.playlists
    }

    fn incoming(&self) -> &Arc<super::incoming::IncomingService> {
        &self.incoming
    }

    fn play_on(&self) -> &Arc<super::playon::PlayOn> {
        &self.play_on
    }

    fn settings(&self) -> &super::settings::SettingsService {
        &self.settings
    }

    fn albums<'a>(&'a self, want: Option<&'a str>) -> Asked<'a, super::AlbumsView> {
        Box::pin(App::albums(self, want))
    }
}
