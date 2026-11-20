//! The playlists as the Settings page asks them (`settings_base::SettingsPlaylists`).

use anyhow::Result;

use super::playlists::PlaylistService;
use super::settings_base::{Later, SettingsPlaylists};

impl SettingsPlaylists for PlaylistService {
    fn count(&self) -> Later<'_, Result<usize>> {
        Box::pin(PlaylistService::count(self))
    }
}
