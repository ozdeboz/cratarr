//! What an album lacks of its smallest release that holds it, and the albums whose rows still fit them, below the areas that read
//! them (moved from `incomplete`, which keeps their names).

use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// What an album lacks of its smallest release that holds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Incomplete {
    /// How many tracks the release has, and the album.
    pub of: usize,
    pub has: usize,
    /// The release, as Renumber shows one ("Delight, 2023, Digital Media").
    pub release: String,
    /// Its release group: what "Ask for the full album" asks Lidarr for.
    #[serde(default)]
    pub release_group: Option<String>,
    /// The release's MusicBrainz id (0.9.8: where single-track replacement finds the missing tracks).
    #[serde(default)]
    pub release_id: Option<String>,
    /// Each missing track: its disc and number on the release (from 1), and its title.
    pub missing: Vec<(u32, u32, String)>,
    /// The release's length and its longest track (ms), where every track's length is known (0.13: the type rule reads
    /// an incomplete album as its release).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub longest_ms: Option<u64>,
}

impl Incomplete {
    /// Whether this row still fits its album (A3.4): its `has` is the album's tracks in the library now. One that no longer does (a
    /// Replace, tracks added or gone since its look) is read by nothing, and the pass looks at its album again at once.
    pub fn fits(&self, present: usize) -> bool {
        self.has == present
    }
}

/// The albums found missing tracks whose rows still fit them (A3.4), each with its row: what Health's records missing tracks, a
/// record's own health rows (and their "Ask for the full album") and the missing tracks' release read. Blocking.
pub fn still_short(served: &crate::serve::Serve) -> Result<Vec<(String, Incomplete)>> {
    let counts = served.library.song_counts()?;
    Ok(fitting(served.userdata.albums_incomplete()?, &counts))
}

/// `still_short`'s rule: the rows that read and fit their album's tracks now (`counts`, by album).
fn fitting(rows: Vec<(String, String)>, counts: &HashMap<String, usize>) -> Vec<(String, Incomplete)> {
    rows.into_iter()
        .filter_map(|(album, json)| {
            let i = serde_json::from_str::<Incomplete>(&json).ok()?;
            i.fits(counts.get(&album).copied().unwrap_or(0)).then_some((album, i))
        })
        .collect()
}
