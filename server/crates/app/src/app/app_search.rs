//! The app as search reads it (`search_host::SearchHost`).

use std::sync::Arc;

use super::App;
use super::search_host::SearchHost;

impl SearchHost for App {
    fn served(&self) -> &Option<Arc<crate::serve::Serve>> {
        &self.served
    }

    fn library(&self) -> &Arc<super::library::Library> {
        &self.library
    }

    fn playlists(&self) -> &Arc<super::playlists::PlaylistService> {
        &self.playlists
    }
}
