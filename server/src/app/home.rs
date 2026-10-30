//! Home's sections that the page used to put together itself, now the server's (the owner, 2026-10-24: "Why is New
//! for you, Jump back in and Sounds like your week not server assembled then?"), so every section keeps the same rules:
//! no album without a cover (the owner the same day), and a queue that is an album's tracks shown as that album.
//!
//! - **New for you**: Cratarr's own library's newest albums (every folder in use), those the account heard 5% or more
//!   of left out, three at most of one album artist; one rule with its crate (`PlaylistService::new_for_you`).
//! - **Jump back in**: the albums played last (14 days, four at most), the playlists played last (30 days, three), and
//!   the asking player's queues left last (3 days, two), purely by when; a queue holding an album's tracks (nine in ten
//!   of them or more, nothing else) is that album.
//! - **Sounds like your week**: the week's crate, its first albums for the mosaic, its size, its genres and how many
//!   of the week's tracks it is drawn from.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use super::{App, ToggleError};
use crate::source;

/// Jump back in, by kind: how far back (days) and how many at most.
const ALBUMS_BACK: (i64, usize) = (14, 4);
const PLAYLISTS_BACK: (i64, usize) = (30, 3);
const QUEUES_BACK: (i64, usize) = (3, 2);
/// A queue is an album's when it holds at least this share of the album's tracks and nothing else.
const ALBUM_SHARE: f64 = 0.9;
/// Sounds like your week's crate, and how many of its rows are read for its mosaic.
const WEEK: &str = "like:week";
const WEEK_ROWS: usize = 40;

#[derive(Debug, Serialize)]
pub struct NewAlbum {
    pub source: String,
    #[serde(flatten)]
    pub album: source::Album,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Back {
    Album { at: i64, source: String, id: String, title: String, by: Option<String> },
    Playlist { at: i64, id: String, title: String },
    Queue { at: i64, id: String, title: String, tracks: usize, from: Option<String> },
}

impl Back {
    fn at(&self) -> i64 {
        match self {
            Back::Album { at, .. } | Back::Playlist { at, .. } | Back::Queue { at, .. } => *at,
        }
    }
}

/// An album of the week's mosaic: its cover is its first track's.
#[derive(Debug, Serialize)]
pub struct WeekCover {
    pub track: String,
    pub album_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Week {
    pub name: String,
    pub source: String,
    pub total: usize,
    pub genres: Vec<String>,
    /// How many of the week's tracks it is drawn from.
    pub played: usize,
    pub covers: Vec<WeekCover>,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// The albums without a cover, of Cratarr's own library (Home shows none of them).
async fn coverless(served: Option<&crate::serve::Serve>) -> HashSet<String> {
    let Some(served) = served else { return HashSet::new() };
    let lib = served.library.clone();
    tokio::task::spawn_blocking(move || lib.coverless()).await.ok().and_then(Result::ok).unwrap_or_default()
}

/// New for you's albums (`picks`, by id) from `albums`, in the picks' order; one `albums` lacks is left out.
fn shelf_of(picks: &[String], albums: impl IntoIterator<Item = source::Album>) -> Vec<source::Album> {
    let mut by_id: HashMap<String, source::Album> = albums.into_iter().map(|a| (a.id.clone(), a)).collect();
    picks.iter().filter_map(|id| by_id.remove(id)).collect()
}

/// The newest `most` within `days`, the latest first.
fn latest(mut list: Vec<Back>, (days, most): (i64, usize), now: i64) -> Vec<Back> {
    list.retain(|b| b.at() > now - days * 86_400);
    list.sort_by_key(|b| std::cmp::Reverse(b.at()));
    list.truncate(most);
    list
}

/// The album a queue's tracks are, when they are (nine in ten of its tracks or more, and nothing else): (id, title,
/// artist).
fn album_of_queue(tracks: &[(String, String)], albums_of: &HashMap<String, String>, albums: &HashMap<String, crate::serve::api::Album>) -> Option<(String, String, String)> {
    let own = crate::source::cratarr::KIND.id;
    if tracks.is_empty() || tracks.iter().any(|(s, _)| s != own) {
        return None;
    }
    let mut ids = tracks.iter().map(|(_, id)| albums_of.get(id));
    let first = ids.next()??;
    if !ids.all(|a| a == Some(first)) {
        return None;
    }
    let album = albums.get(first)?;
    let distinct: HashSet<&String> = tracks.iter().map(|(_, id)| id).collect();
    (distinct.len() as f64 >= ALBUM_SHARE * f64::from(album.song_count)).then(|| (album.id.clone(), album.name.clone(), album.artist.clone()))
}

impl App {
    /// Home's New for you: its crate's albums (`PlaylistService::new_for_you`), in its order, as Cratarr's own album
    /// lists have them; one the lists have not read yet, as the library has it.
    pub async fn home_new(&self) -> Result<Vec<NewAlbum>, ToggleError> {
        let picks = self.playlists.new_for_you().await?;
        if picks.is_empty() {
            return Ok(Vec::new());
        }
        let own = crate::source::cratarr::KIND.id;
        let listed: Vec<source::Album> = self.albums(Some(own)).await.sources.into_iter().flat_map(|s| s.albums).collect();
        let unlisted: Vec<String> = picks.iter().filter(|id| !listed.iter().any(|a| &a.id == *id)).cloned().collect();
        let late: Vec<source::Album> = match (&self.served, unlisted.is_empty()) {
            (Some(served), false) => {
                let lib = served.library.clone();
                let found = tokio::task::spawn_blocking(move || lib.albums_by_id(&unlisted)).await.map_err(|e| anyhow::anyhow!("{e}"))??;
                found.into_iter().map(crate::source::cratarr::card).collect()
            }
            _ => Vec::new(),
        };
        Ok(shelf_of(&picks, listed.into_iter().chain(late)).into_iter().map(|album| NewAlbum { source: own.to_string(), album }).collect())
    }

    /// Home's Jump back in, for the asking `player` (its queues).
    pub async fn home_back(&self, player: Option<&str>) -> Result<Vec<Back>, ToggleError> {
        let at = now();
        let own = crate::source::cratarr::KIND.id;
        let coverless = coverless(self.served.as_deref()).await;
        let view = self.albums(None).await;
        // The albums played last, each server's plays.
        let mut albums: Vec<Back> = Vec::new();
        for s in &view.sources {
            let Ok(uses) = self.playing.album_uses(&s.id).await else { continue };
            for a in &s.albums {
                if s.id == own && coverless.contains(&a.id) {
                    continue;
                }
                if let Some(played) = uses.get(&a.id).and_then(|u| u.played) {
                    albums.push(Back::Album { at: played, source: s.id.clone(), id: a.id.clone(), title: a.title.clone(), by: a.artist.clone() });
                }
            }
        }
        // The playlists played last.
        let stats = self.playlists.playlist_stats().await?;
        let lists: Vec<Back> = self
            .playlists
            .view(None)
            .await
            .map_err(ToggleError::from)?
            .into_iter()
            .filter_map(|p| Some(Back::Playlist { at: stats.get(&p.id)?.last_played?, id: p.id, title: p.name }))
            .collect();
        // The queues left; one that is an album's tracks is that album.
        let left = player.map(|p| self.play_on.left_queues(p)).unwrap_or_default();
        let mut queues: Vec<Back> = Vec::new();
        if !left.is_empty() {
            let ids: Vec<String> = left.iter().flat_map(|(_, t)| t.iter().filter(|(s, _)| s == own).map(|(_, id)| id.clone())).collect();
            let found = match self.served.clone() {
                Some(served) => {
                    let lib = served.library.clone();
                    tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
                        let albums_of = lib.albums_of(&ids)?;
                        let album_ids: Vec<String> = albums_of.values().cloned().collect::<HashSet<_>>().into_iter().collect();
                        let albums: HashMap<String, crate::serve::api::Album> = lib.albums_by_id(&album_ids)?.into_iter().map(|a| (a.id.clone(), a)).collect();
                        Ok((albums_of, albums))
                    })
                    .await
                    .ok()
                    .and_then(Result::ok)
                }
                None => None,
            };
            for (q, tracks) in left {
                let at = q.updated as i64;
                match found.as_ref().and_then(|(of, albums)| album_of_queue(&tracks, of, albums)) {
                    Some((id, title, by)) if !coverless.contains(&id) => {
                        albums.push(Back::Album { at, source: own.to_string(), id, title, by: Some(by) });
                    }
                    Some(_) => {}
                    None => queues.push(Back::Queue { at, id: q.id, title: q.name, tracks: q.tracks, from: q.from }),
                }
            }
        }
        // An album once, at its latest.
        albums.sort_by_key(|b| std::cmp::Reverse(b.at()));
        let mut seen = HashSet::new();
        albums.retain(|b| match b {
            Back::Album { source, id, .. } => seen.insert((source.clone(), id.clone())),
            _ => true,
        });
        let mut out = [latest(albums, ALBUMS_BACK, at), latest(lists, PLAYLISTS_BACK, at), latest(queues, QUEUES_BACK, at)].concat();
        out.sort_by_key(|b| std::cmp::Reverse(b.at()));
        Ok(out)
    }

    /// Home's Sounds like your week, of the libraries switched on (`without`); none while it has no tracks.
    pub async fn home_week(&self, want: Option<&str>, without: Vec<String>) -> Result<Option<Week>, ToggleError> {
        let def = self.playlists.crate_def(WEEK).await?;
        let played = def
            .conditions
            .iter()
            .find_map(|c| match &c.value {
                crate::crates::Value::Like { of, weights, .. } => Some(of.iter().enumerate().filter(|(i, _)| weights.get(*i).copied().unwrap_or(1.0) > 0.0).count()),
                _ => None,
            })
            .unwrap_or(0);
        let look = super::CrateLook { without, ..Default::default() };
        let page = self.playlists.crate_rows(WEEK, want, 0, WEEK_ROWS, &look).await?;
        if page.total == 0 {
            return Ok(None);
        }
        let coverless = coverless(self.served.as_deref()).await;
        let mut seen = HashSet::new();
        let mut covers = Vec::new();
        for r in &page.rows {
            let Some(track) = r.id.clone().filter(|_| !r.missing) else { continue };
            if r.album_id.as_ref().is_some_and(|a| coverless.contains(a)) {
                continue;
            }
            let key = r.album_id.clone().unwrap_or_else(|| format!("{}\n{}", r.album.clone().unwrap_or_default(), r.album_artist.clone().or(r.artist.clone()).unwrap_or_default()));
            if !seen.insert(key) {
                continue;
            }
            covers.push(WeekCover { track, album_id: r.album_id.clone() });
            if covers.len() == 4 {
                break;
            }
        }
        Ok(Some(Week { name: def.name, source: page.source, total: page.total, genres: page.genres.into_iter().take(4).collect(), played, covers }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn album(id: &str, songs: u32) -> crate::serve::api::Album {
        crate::serve::api::Album { id: id.into(), name: format!("Album {id}"), artist: "Air".into(), song_count: songs, ..Default::default() }
    }

    #[test]
    fn a_queue_of_an_albums_tracks_is_that_album() {
        let own = crate::source::cratarr::KIND.id.to_string();
        let t = |id: &str| (own.clone(), id.to_string());
        let of: HashMap<String, String> = [("t1", "a"), ("t2", "a"), ("t3", "a"), ("t4", "a"), ("x", "b")].into_iter().map(|(t, a)| (t.into(), a.into())).collect();
        let albums: HashMap<String, crate::serve::api::Album> = [("a".to_string(), album("a", 4)), ("b".to_string(), album("b", 10))].into_iter().collect();
        assert_eq!(album_of_queue(&[t("t1"), t("t2"), t("t3"), t("t4")], &of, &albums).map(|a| a.0), Some("a".into()), "the whole album");
        assert_eq!(album_of_queue(&[t("t1"), t("t2")], &of, &albums), None, "half of it is a queue");
        assert_eq!(album_of_queue(&[t("t1"), t("t2"), t("t3"), t("t4"), t("x")], &of, &albums), None, "with another album's track");
        assert_eq!(album_of_queue(&[("jellyfin".into(), "t1".into())], &of, &albums), None, "another server's");
    }

    #[test]
    fn jump_back_in_keeps_each_kinds_window_and_count() {
        let now = 1_000_000;
        let a = |at: i64, id: &str| Back::Album { at, source: "cratarr".into(), id: id.into(), title: id.into(), by: None };
        let got = latest(vec![a(now - 100, "x"), a(now - 15 * 86_400, "old"), a(now - 50, "y"), a(now - 10, "z"), a(now - 20, "w"), a(now - 30, "v")], ALBUMS_BACK, now);
        let ids: Vec<String> = got.iter().map(|b| if let Back::Album { id, .. } = b { id.clone() } else { String::new() }).collect();
        assert_eq!(ids, ["z", "w", "v", "y"], "the latest four within 14 days");
    }

    /// The shelf and its crate are one rule (`PlaylistService::new_for_you`): the shelf shows the crate's albums in the
    /// crate's order, whatever order the lists hold them in and whatever else they hold; one the lists have not read yet
    /// comes from the library.
    #[test]
    fn the_shelf_shows_its_crates_albums_in_the_crates_order() {
        let picks = vec!["c".to_string(), "a".to_string(), "late".to_string()];
        let listed = ["a", "b", "c", "d"].map(|id| crate::source::cratarr::card(album(id, 3)));
        let late = [crate::source::cratarr::card(album("late", 2))];
        let ids: Vec<String> = shelf_of(&picks, listed.into_iter().chain(late)).into_iter().map(|a| a.id).collect();
        assert_eq!(ids, picks);
        assert!(shelf_of(&picks, Vec::<source::Album>::new()).is_empty(), "none the lists or the library have");
    }
}
