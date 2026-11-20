//! `matching`'s match, in base: an album's match as kept, which records' references read and keep. `matching` re-exports it.

use serde::{Deserialize, Serialize};

/// An album's match, as kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Match {
    pub release: String,
    pub release_group: Option<String>,
    pub title: String,
    /// Its artist credit as MusicBrainz writes it, and its first artist's id.
    pub artist: String,
    pub artist_id: Option<String>,
    pub year: Option<u32>,
    pub certain: bool,
    pub why: String,
    /// Each of the album's files (by name) with its recording and its first artist's id.
    pub tracks: Vec<(String, String, Option<String>)>,
    /// Each one's length on the release (s), as `tracks` (the intake card's reference lengths, 0.14).
    #[serde(default)]
    pub lengths: Vec<Option<i64>>,
    /// The release's format ("CD", "Digital Media"), as its words name it; none in a match kept before.
    #[serde(default)]
    pub format: String,
    /// How many of MusicBrainz's editions compared fit with every track within `LIKELY_SECS` (its words: "several fit").
    #[serde(default)]
    pub fits: usize,
}
