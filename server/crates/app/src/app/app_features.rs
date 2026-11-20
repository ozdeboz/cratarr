//! The app as search's and Health's functions read it (`features_host::FeaturesHost`).

use std::sync::Arc;

use super::App;
use super::features_host::{Asked, FeaturesHost};

impl FeaturesHost for App {
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
