//! The playlists as insights ask them (`insights_records::InsightsPlaylists`).

use std::collections::HashSet;
use std::sync::Arc;

use super::ToggleError;
use super::insights_records::InsightsPlaylists;
use super::playlists::PlaylistService;
use super::settings_base::Later;

impl InsightsPlaylists for PlaylistService {
    fn asking_account(&self) -> Option<String> {
        PlaylistService::asking_account(self)
    }

    fn served(&self) -> Option<Arc<crate::serve::Serve>> {
        PlaylistService::served(self)
    }

    fn crated_lines<'a>(&'a self, want: Option<&'a str>, without: &'a [String]) -> Later<'a, Result<HashSet<String>, ToggleError>> {
        Box::pin(PlaylistService::crated_lines(self, want, without))
    }
}
