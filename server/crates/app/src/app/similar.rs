//! Lists like a list, of its own kind (the owner, 2026-10-23: "any list that has a profile at the top gets a similarity
//! set at the bottom"). An album's albums are `radio::similar_albums`, a playlist's playlists
//! `PlaylistService::similar_playlists`; a crate's crates are here: the asking account's crates, each by the tracks it
//! holds (its first page, newest first), through the one similarity (`radio::similar_lists`). A crate made from a seed
//! (a genre, Sounds like) has none: it is no crate of one's own.

use super::{CrateLook, PlaylistService, ToggleError};
use crate::crates::{GENRE, LIKE};

/// How many of a crate's tracks stand for it: one page.
const SAMPLE: usize = 500;

fn seeded(id: &str) -> bool {
    id.starts_with(GENRE) || id.starts_with(LIKE)
}

/// A crate's tracks (Cratarr's own song ids), as saved, from every library: its first `SAMPLE`.
async fn songs_of(playlists: &PlaylistService, id: &str) -> Result<Vec<String>, ToggleError> {
    let page = playlists.crate_rows(id, Some(crate::source::cratarr::KIND.id), 0, SAMPLE, &CrateLook::default()).await?;
    Ok(page.rows.into_iter().filter_map(|r| r.id).collect())
}

/// Crates like a crate, among the asking account's, the closest first, with how alike (0 to 100).
pub async fn crates_like(playlists: &PlaylistService, id: &str, count: usize) -> Result<Vec<(String, f64)>, ToggleError> {
    if seeded(id) {
        return Ok(Vec::new());
    }
    let seed = songs_of(playlists, id).await?;
    let mut candidates = Vec::new();
    for c in playlists.crates(Some(crate::source::cratarr::KIND.id), &[]).await? {
        if c.id == id || seeded(&c.id) {
            continue;
        }
        // One that cannot be read now (its favourites' server away) is left out.
        if let Ok(songs) = songs_of(playlists, &c.id).await {
            candidates.push((c.id, songs));
        }
    }
    super::radio::similar_lists(seed, candidates, count).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seeded_crate_is_none_of_ones_own() {
        assert!(seeded("genre:Ambient") && seeded("like:tr-1@50"));
        assert!(!seeded("c-1"));
    }
}
