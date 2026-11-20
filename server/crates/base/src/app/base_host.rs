//! What base's functions read of the app (App's wiring, the app split's phase 1): the services they use, as the app holds
//! them, and what they ask of the one path's runs above base. The app implements it (`app_base`); a function takes
//! `app: &Arc<H>` with `H: BaseHost`, so its callers pass the app as before, and base never names `App`.

use std::sync::Arc;

use super::file_runs_base::{Proposal, Status};

pub trait BaseHost: Send + Sync + 'static {
    /// The servers in use, the folders behind their libraries, the catalogue (`App::library`).
    fn library(&self) -> &Arc<super::library::Library>;
    /// The Settings page (`App::settings`).
    fn settings(&self) -> &super::settings::SettingsService;
    /// The asking account's own folder of playlists (`PlaylistService::own_folder`).
    fn own_folder(&self) -> String;
    /// Whether a writer has a run waiting or writing (`file_runs::busy`, in `App::journal`).
    fn run_busy(&self, what: &str) -> bool;
    /// A proposal run on the one path (`file_runs::start`).
    fn start_run(self: Arc<Self>, p: Proposal) -> Status;
}
