//! The playlist service as the server holds it (`serve::services::Playlists`): what the server asks of it, and the service
//! taken back as its own type by code above that knows it (`PlaylistService::held_by`).

use std::any::Any;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;

use super::playlists::PlaylistService;
use crate::serve::Serve;
use crate::serve::services::Playlists;

impl Playlists for PlaylistService {
    fn follow_tracks<'a>(&'a self, moved: &'a crate::moved::Moved) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>> {
        Box::pin(PlaylistService::follow_tracks(self, moved))
    }

    fn as_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}

impl PlaylistService {
    /// The server's playlist service (`Serve::playlists`) as this service: none until the app hands it over.
    pub fn held_by(serve: &Serve) -> Option<Arc<Self>> {
        serve.playlists().cloned().and_then(|p| p.as_any().downcast::<Self>().ok())
    }
}
