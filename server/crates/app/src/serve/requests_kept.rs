//! Who asked for what, as the server keeps it last read (`Serve::requesters_kept`), below the app (moved from `app::requests`,
//! which keeps their names and reads them: `requesters`).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

/// Who asked for what, as last read, and when (`requesters`): on the server, so each server of the tests has its own.
#[derive(Debug, Default)]
pub struct KeptRequesters(pub Option<(Instant, Arc<HashMap<i64, Requester>>)>);

/// Who asked for a record, and when (`requesters`).
#[derive(Debug, Clone, PartialEq)]
pub struct Requester {
    pub user: String,
    /// When they asked, epoch seconds.
    pub at: i64,
}

impl Requester {
    /// Whether a copy that came at `came` is theirs to keep into their own music or to delete: it came after they asked (R32,
    /// N14, the owner's). One the library held already (an upgrade the administrator's Lidarr fetched, an album that was there
    /// when they asked), or whose coming is not known, stays an administrator's.
    pub fn of_copy(&self, came: Option<i64>) -> bool {
        came.is_some_and(|came| came >= self.at)
    }
}
