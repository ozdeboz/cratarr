//! `playon::along`'s followed player, in base: what a player's view names it follows. `along` re-exports it.

use serde::{Deserialize, Serialize};

/// A player followed, by its user and name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct FollowTarget {
    pub user: String,
    pub player: String,
}
