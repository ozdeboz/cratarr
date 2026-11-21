//! Incoming as the Settings page asks it (`settings_base::SettingsIncoming`).

use super::incoming::IncomingService;
use super::incoming_base::LidarrPair;
use super::settings_base::{Later, SettingsIncoming};

impl SettingsIncoming for IncomingService {
    fn lidarr_state(&self) -> LidarrPair {
        IncomingService::lidarr_state(self)
    }

    fn trash_days(&self) -> u32 {
        IncomingService::trash_days(self)
    }

    fn apply_settings(&self, lidarr: LidarrPair, trash_days: u32) {
        IncomingService::apply_settings(self, lidarr, trash_days)
    }

    fn forget_lists(&self) -> Later<'_, ()> {
        Box::pin(IncomingService::forget_lists(self))
    }

    fn layouts_known(&self) {
        IncomingService::wake_queue(self)
    }
}
