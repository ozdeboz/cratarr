//! Incoming as the server holds it (`serve::services::Incoming`): what the server asks of it, and the service taken back as
//! its own type by code above that knows it (`IncomingService::held_by`).

use std::any::Any;
use std::sync::Arc;

use super::incoming::IncomingService;
use crate::serve::Serve;
use crate::serve::services::Incoming;

impl Incoming for IncomingService {
    fn folder_kinds(&self) -> Vec<(i64, crate::settings::LibraryKind)> {
        IncomingService::folder_kinds(self)
    }

    fn folders_off(&self, without: &[String]) -> Vec<i64> {
        IncomingService::folders_off(self, without)
    }

    fn folder_rules(&self) -> crate::catalogue::FolderRules {
        IncomingService::folder_rules(self)
    }

    fn as_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}

impl IncomingService {
    /// The server's Incoming (`Serve::incoming`) as this service: none until the app hands it over.
    pub fn held_by(serve: &Serve) -> Option<Arc<Self>> {
        serve.incoming().cloned().and_then(|i| i.as_any().downcast::<Self>().ok())
    }
}
