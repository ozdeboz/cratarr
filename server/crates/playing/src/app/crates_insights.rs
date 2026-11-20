//! The crates as insights ask them (`insights_records::InsightsPlaylists`).

use std::collections::HashSet;
use std::sync::Arc;

use super::ToggleError;
use super::crates::CrateService;
use super::insights_records::InsightsPlaylists;
use super::settings_base::Later;

impl InsightsPlaylists for CrateService {
    fn asking_account(&self) -> Option<String> {
        self.playlists().asking_account()
    }

    fn served(&self) -> Option<Arc<crate::serve::Serve>> {
        self.playlists().served()
    }

    fn crated_lines<'a>(&'a self, want: Option<&'a str>, without: &'a [String]) -> Later<'a, Result<HashSet<String>, ToggleError>> {
        Box::pin(CrateService::crated_lines(self, want, without))
    }
}
