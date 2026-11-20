//! The app as playing's functions read it (`playing_host::PlayingHost`).

use std::sync::Arc;

use super::App;
use super::playing_host::PlayingHost;

impl PlayingHost for App {
    fn library(&self) -> &Arc<super::library::Library> {
        &self.library
    }

    fn playlists(&self) -> &Arc<super::playlists::PlaylistService> {
        &self.playlists
    }
}
