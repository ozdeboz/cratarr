//! Lists like a list, of its own kind (the owner, 2026-10-23: "any list that has a profile at the top gets a similarity
//! set at the bottom"). An album's albums are `radio::similar_albums`; a playlist's playlists and a crate's crates are
//! here: the asking account's crates, each by the tracks it holds (its first page, newest first), through the one
//! similarity (`radio::similar_lists`). A crate made from a seed (a genre, Sounds like) has none: it is no crate of one's
//! own. Here in playing, beside radio, whose index the one similarity reads (SP3-U: `similar_playlists` was the playlist
//! service's).

use super::{CrateLook, CrateService, PlaylistService, ToggleError};
use crate::crates::{GENRE, LIKE};

/// How many of a crate's tracks stand for it: one page.
const SAMPLE: usize = 500;

fn seeded(id: &str) -> bool {
    id.starts_with(GENRE) || id.starts_with(LIKE)
}

/// A crate's tracks (Cratarr's own song ids), as saved, from every library: its first `SAMPLE`.
async fn songs_of(crates: &CrateService, id: &str) -> Result<Vec<String>, ToggleError> {
    let page = crates.crate_rows(id, Some(crate::source::cratarr::KIND.id), 0, SAMPLE, &CrateLook::default()).await?;
    Ok(page.rows.into_iter().filter_map(|r| r.id).collect())
}

/// Playlists like a playlist (one's own or a shared one; the owner, 2026-10-23: same kind under each list), among
/// those the asking account sees, the closest first, with how alike (0 to 100): radio::similar_lists.
pub async fn similar_playlists(playlists: &PlaylistService, id: &str, count: usize) -> Result<Vec<(String, f64)>, ToggleError> {
    let Some(serve) = playlists.served() else { return Ok(Vec::new()) };
    let lists = playlists.visible_lines().await?;
    let id = id.to_string();
    let (seed, candidates) = crate::run::spawn_blocking(move || {
        let cat = serve.catalogue();
        let songs = |lines: &[String]| lines.iter().filter_map(|l| crate::serve::playlists::song_of_line(cat, l)).collect::<Vec<_>>();
        let mut seed = Vec::new();
        let mut candidates = Vec::new();
        for (list, _, lines) in &lists {
            if *list == id {
                seed = songs(lines);
            } else {
                candidates.push((list.clone(), songs(lines)));
            }
        }
        (seed, candidates)
    })
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    super::radio::similar_lists(seed, candidates, count).await
}

/// Crates like a crate, among the asking account's, the closest first, with how alike (0 to 100).
pub async fn crates_like(crates: &CrateService, id: &str, count: usize) -> Result<Vec<(String, f64)>, ToggleError> {
    if seeded(id) {
        return Ok(Vec::new());
    }
    let seed = songs_of(crates, id).await?;
    let mut candidates = Vec::new();
    for c in crates.crates(Some(crate::source::cratarr::KIND.id), &[]).await? {
        if c.id == id || seeded(&c.id) {
            continue;
        }
        // One that cannot be read now (its favourites' server away) is left out.
        if let Ok(songs) = songs_of(crates, &c.id).await {
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
