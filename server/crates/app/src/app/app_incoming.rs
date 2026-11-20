//! The app as incoming's functions read it (`incoming_host::IncomingHost`).

use std::sync::Arc;

use anyhow::Result;

use super::App;
use super::incoming_host::{Asked, IncomingHost};

impl IncomingHost for App {
    fn library(&self) -> &Arc<super::library::Library> {
        &self.library
    }

    fn incoming(&self) -> &Arc<super::incoming::IncomingService> {
        &self.incoming
    }

    fn journal(&self) -> &Arc<crate::journal::Journal> {
        &self.journal
    }

    fn served(&self) -> &Option<Arc<crate::serve::Serve>> {
        &self.served
    }

    fn add_fetched(self: Arc<Self>, fetch: String) -> Asked<'static, Result<super::file_runs::Status>> {
        Box::pin(async move { super::track_fetch::add(&self, &fetch).await })
    }
}
