//! What the Settings page asks of Incoming and of the playlists, below them (the app split's phase 1): Lidarr's connection
//! and the trash days, and how many playlists there are. `IncomingService` and `PlaylistService` implement it
//! (`incoming_settings`, `playlists_settings`); `SettingsService` holds them as these.

use std::future::Future;
use std::pin::Pin;

use anyhow::Result;

use super::incoming_base::LidarrPair;

/// A future a service hands back, as the Settings page awaits it (inside the HTTP handlers: `Send`).
pub type Later<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait SettingsIncoming: Send + Sync {
    /// The saved Lidarr connection and its client.
    fn lidarr_state(&self) -> LidarrPair;
    fn trash_days(&self) -> u32;
    /// Take saved settings into use: the Lidarr connection and the trash days.
    fn apply_settings(&self, lidarr: LidarrPair, trash_days: u32);
    /// Servers and libraries may have changed: the lists are read again on the next look.
    fn forget_lists(&self) -> Later<'_, ()>;
}

pub trait SettingsPlaylists: Send + Sync {
    /// 0 when there is no Music folder.
    fn count(&self) -> Later<'_, Result<usize>>;
}
