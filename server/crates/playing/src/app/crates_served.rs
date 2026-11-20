//! The crate service as the server holds it (`serve::services::Playlists`): what the server asks of it, and the service
//! taken back as its own type by code above that knows it (`CrateService::held_by`; the playlists through it,
//! `CrateService::playlists`).

use std::any::Any;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;

use super::crates::CrateService;
use crate::serve::Serve;
use crate::serve::services::Playlists;

impl Playlists for CrateService {
    fn follow_tracks<'a>(&'a self, moved: &'a crate::moved::Moved) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>> {
        Box::pin(CrateService::follow_tracks(self, moved))
    }

    fn as_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}

impl CrateService {
    /// The server's crate service (`Serve::playlists`) as this service: none until the app hands it over.
    pub fn held_by(serve: &Serve) -> Option<Arc<Self>> {
        serve.playlists().cloned().and_then(|p| p.as_any().downcast::<Self>().ok())
    }
}
