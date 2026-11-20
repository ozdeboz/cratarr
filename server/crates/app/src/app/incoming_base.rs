//! `incoming`'s Lidarr view and connection, in base: Lidarr as the Settings page shows it, and a connection with its
//! client, as base's settings make them. `incoming` re-exports them.

use serde::Serialize;

use crate::settings;

/// Lidarr on the Settings page. The key is never sent.
#[derive(Debug, Clone, Serialize)]
pub struct LidarrView {
    pub url: String,
    pub key_set: bool,
    /// Switched on; off, not asked.
    pub enabled: bool,
    /// What Discover's Want adds an album with (`settings::LidarrConfig`); none: as the artists Lidarr has.
    pub quality_profile: Option<i64>,
    pub metadata_profile: Option<i64>,
    pub root_folder: Option<String>,
    /// Lidarr's version when it answered.
    pub version: Option<String>,
    pub error: Option<String>,
}

/// A Lidarr connection and its client (`None` when the key cannot be used in a header).
pub(super) type LidarrPair = (Option<settings::LidarrConfig>, Option<crate::lidarr::Lidarr>);

pub(super) fn lidarr_pair(cfg: Option<settings::LidarrConfig>) -> LidarrPair {
    // Switched off: kept, not used.
    let client = cfg.as_ref().filter(|c| c.enabled).and_then(|c| match crate::lidarr::Lidarr::new(&c.url, &c.api_key) {
        Ok(l) => Some(l),
        Err(e) => {
            tracing::warn!("Lidarr: {e:#}");
            None
        }
    });
    (cfg, client)
}
