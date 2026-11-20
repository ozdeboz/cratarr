//! The app as playing's functions read it (`playing_host::PlayingHost`).

use std::sync::Arc;

use super::App;
use super::playing_host::{Asked, PlayingHost};

impl PlayingHost for App {
    fn library(&self) -> &Arc<super::library::Library> {
        &self.library
    }

    fn playlists(&self) -> &Arc<super::playlists::PlaylistService> {
        &self.playlists
    }

    fn playing(&self) -> &super::playing::PlayingService {
        &self.playing
    }

    fn play_on(&self) -> &Arc<super::playon::PlayOn> {
        &self.play_on
    }

    fn visuals(&self) -> &Arc<super::visuals::GroupVisuals> {
        &self.visuals
    }

    fn served(&self) -> &Option<Arc<crate::serve::Serve>> {
        &self.served
    }

    fn album_moved_to<'a>(&'a self, source: &'a str, id: &'a str) -> Asked<'a, Option<String>> {
        Box::pin(async move {
            match self.incoming.album_fate(source, id).await {
                Some(super::AlbumFate::Moved(now)) => Some(now),
                _ => None,
            }
        })
    }
}
