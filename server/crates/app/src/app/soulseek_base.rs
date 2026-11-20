//! Soulseek's word of a request's record, in base: a request's state (`requests_base::RequestState`) holds it, as the pages show
//! it. `soulseek` re-exports it and says it.

use serde::Serialize;

/// What Soulseek says of a request's record (W5): its item's state, as the request reads it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Word {
    /// Being searched now.
    Searching,
    /// Folders found, waiting for the administrator's tick.
    Found,
    /// Ticked and fetched (how far, 0 to 100), or its files checked and placed.
    Fetching { percent: f64 },
    /// To be searched, again where a search found nothing: why, where it says.
    Waiting {
        #[serde(skip_serializing_if = "Option::is_none")]
        why: Option<String>,
    },
    /// Something it cannot get past by itself (no list to judge it by), and why.
    Stuck {
        #[serde(skip_serializing_if = "Option::is_none")]
        why: Option<String>,
    },
}
