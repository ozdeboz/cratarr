//! What insights ask of the playlists, below them (the app split's phase 1): whose report it is, Cratarr's own library, and
//! the tracks in the asking account's crates. `CrateService` implements it (`crates_insights`); `InsightsService` holds
//! it as this.

use std::collections::HashSet;
use std::sync::Arc;

use super::ToggleError;
use super::settings_base::Later;

pub trait InsightsPlaylists: Send + Sync {
    fn asking_account(&self) -> Option<String>;
    fn served(&self) -> Option<Arc<crate::serve::Serve>>;
    fn crated_lines<'a>(&'a self, want: Option<&'a str>, without: &'a [String]) -> Later<'a, Result<HashSet<String>, ToggleError>>;
}
