//! The players as the server holds them (`serve::services::Players`), taken back as their own type by code above that knows
//! them (`PlayOn::held_by`).

use std::any::Any;
use std::sync::Arc;

use super::playon::PlayOn;
use crate::serve::Serve;
use crate::serve::services::Players;

impl Players for PlayOn {
    fn as_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }
}

impl PlayOn {
    /// The server's players (`Serve::play_on`) as Play On: none until the app hands them over.
    pub fn held_by(serve: &Serve) -> Option<Arc<Self>> {
        serve.play_on().cloned().and_then(|p| p.as_any().downcast::<Self>().ok())
    }
}
