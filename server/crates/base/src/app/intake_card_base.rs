//! `intake_card`'s hearing of a copy, in base: what hearing a record's copy found, which records' references read.
//! `intake_card` re-exports it.

use serde::{Deserialize, Serialize};

/// What hearing a record's copy found, kept per copy (`intake_heard`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Heard {
    pub tracks: Vec<HeardTrack>,
    /// AcoustID was not asked (no key set).
    #[serde(default)]
    pub no_acoustid: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HeardTrack {
    /// Its file's name.
    pub file: String,
    pub damaged: u32,
    /// What its sound was heard to have wrong with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flaw: Option<String>,
    /// Where its first flaw is (ms), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flaw_at_ms: Option<i64>,
    /// The recordings AcoustID is sure of (its score at least `acoustid::SURE`).
    #[serde(default)]
    pub recordings: Vec<String>,
    /// It could not be read at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
}
