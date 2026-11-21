//! An arrival's job (SCENARIOS-F53 F53.7, LK.B1): plumbing at the base's behaviour, where an arrival's matching is its card's GET.

use std::sync::Arc;

use super::references_door::References;

/// The job's kind: matching's.
pub const KIND: &str = super::matching::KIND;

/// Whether the arrival's job asked `source` about the record `subject`: never today.
pub fn searched(_ud: &crate::userdata::UserData, _subject: &str, _source: &str) -> bool {
    false
}

/// The arrival's job for `record`: none today.
pub async fn begin(_served: Arc<crate::serve::Serve>, _record: i64) -> Option<crate::runner::Task> {
    None
}

/// [`begin`], asking through `door`: none today.
pub async fn begin_through(_served: Arc<crate::serve::Serve>, _record: i64, _door: Arc<dyn References>) -> Option<crate::runner::Task> {
    None
}

/// The arrivals Cratarr knows now, their jobs begun: none today.
pub async fn arrivals_found(_served: Arc<crate::serve::Serve>) -> Vec<crate::runner::Task> {
    Vec::new()
}

/// The record pass's arrivals: nothing today.
pub fn watch(_served: Arc<crate::serve::Serve>) {}
