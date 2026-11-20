//! The app as base's functions read it (`base_host::BaseHost`).

use std::sync::Arc;

use super::App;
use super::base_host::BaseHost;
use super::file_runs_base::{Proposal, Status};

impl BaseHost for App {
    fn library(&self) -> &Arc<super::library::Library> {
        &self.library
    }

    fn settings(&self) -> &super::settings::SettingsService {
        &self.settings
    }

    fn own_folder(&self) -> String {
        self.playlists.own_folder()
    }

    fn run_busy(&self, what: &str) -> bool {
        super::file_runs::busy(&self.journal, what)
    }

    fn start_run(self: Arc<Self>, p: Proposal) -> Status {
        super::file_runs::start(&self, p)
    }
}
