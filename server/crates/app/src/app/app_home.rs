//! Home's sections as the app answers them (moved from `app::home`, which keeps their rules and shapes): New for you, Jump
//! back in, Sounds like your week and a crate's covers read the playlists, Incoming's and the Library's lists and the players,
//! all of them the app's.

use std::collections::{HashMap, HashSet};

use super::home::{
    ALBUMS_BACK, BACK, Back, COVER_ROWS, COVER_ROWS_AT_MOST, NewAlbum, PLAYLISTS_BACK, QUEUES_BACK, WEEK, WEEK_ROWS, Week, WeekCover, album_of_queue,
    back_key, coverless, latest, listed, now, pick_covers, shelf_of,
};
use super::{App, ToggleError};
use crate::source;

impl App {
    /// Home's New for you: its crate's albums (`CrateService::new_for_you`), in its order, as Cratarr's own album
    /// lists have them; one the lists have not read yet, as the library has it.
    pub async fn home_new(&self) -> Result<Vec<NewAlbum>, ToggleError> {
        let picks = self.crates.new_for_you().await?;
        if picks.is_empty() {
            return Ok(Vec::new());
        }
        let own = crate::source::cratarr::KIND.id;
        let listed: Vec<source::Album> = self.albums(Some(own)).await.sources.into_iter().flat_map(|s| s.albums).collect();
        let unlisted: Vec<String> = picks.iter().filter(|id| !listed.iter().any(|a| &a.id == *id)).cloned().collect();
        let late: Vec<source::Album> = match (&self.served, unlisted.is_empty()) {
            (Some(served), false) => {
                let lib = served.library.clone();
                let found = crate::run::spawn_blocking(move || lib.albums_by_id(&unlisted)).await.map_err(|e| anyhow::anyhow!("{e}"))??;
                found.into_iter().map(crate::source::cratarr::card).collect()
            }
            _ => Vec::new(),
        };
        Ok(shelf_of(&picks, listed.into_iter().chain(late)).into_iter().map(|album| NewAlbum { source: own.to_string(), album }).collect())
    }

    /// Home's Jump back in, for the asking `player` (its queues): built once for those asking at once, kept 20 seconds
    /// for each account and player (0.12.3: Home's Jump back in asked dozens of times a minute, 2026-10-31).
    pub async fn home_back(&self, player: Option<&str>) -> Result<Vec<Back>, ToggleError> {
        let key = back_key(self.served.as_deref(), player);
        let answer = BACK.kept_or_built(key, || self.build_back(player)).await?;
        Ok((*answer).clone())
    }

    /// A play of the running request's account was reported: its Jump back in is built again at the next ask.
    pub fn home_back_heard(&self) {
        BACK.forget(self.served.as_deref().and_then(|s| s.current_user()).as_deref());
    }

    /// Jump back in, built.
    async fn build_back(&self, player: Option<&str>) -> Result<Vec<Back>, ToggleError> {
        let at = now();
        let own = crate::source::cratarr::KIND.id;
        let coverless = coverless(self.served.as_deref()).await;
        // The Library's one list's servers and albums, read from Music's and the other libraries' kept lists without
        // making that whole list (`listed`; 0.12.3: Home's Jump back in asked dozens of times a minute, 2026-10-31).
        let (collection, incoming) = tokio::join!(self.collection.list(None), self.incoming.list(None));
        let music: Vec<(&str, &[source::Album])> = collection.sources.iter().map(|s| (s.id.as_str(), s.albums.as_slice())).collect();
        let other: Vec<(&str, &[source::Album])> = incoming.sources.iter().map(|s| (s.id.as_str(), s.albums.as_slice())).collect();
        // The albums played last, each server's plays.
        let mut albums: Vec<Back> = Vec::new();
        for (server, list) in listed(&music, &other) {
            let Ok(uses) = self.playing.album_uses(server).await else { continue };
            for a in list {
                if server == own && coverless.contains(&a.id) {
                    continue;
                }
                if let Some(played) = uses.get(&a.id).and_then(|u| u.played) {
                    albums.push(Back::Album { at: played, source: server.to_string(), id: a.id.clone(), title: a.title.clone(), by: a.artist.clone() });
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
                    crate::run::spawn_blocking(move || -> anyhow::Result<_> {
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
        let def = self.crates.crate_def(WEEK).await?;
        let played = def
            .conditions
            .iter()
            .find_map(|c| match &c.value {
                crate::crates::Value::Like { of, weights, .. } => Some(of.iter().enumerate().filter(|(i, _)| weights.get(*i).copied().unwrap_or(1.0) > 0.0).count()),
                _ => None,
            })
            .unwrap_or(0);
        let look = super::CrateLook { without, ..Default::default() };
        let page = self.crates.crate_rows(WEEK, want, 0, WEEK_ROWS, &look).await?;
        if page.total == 0 {
            return Ok(None);
        }
        let coverless = coverless(self.served.as_deref()).await;
        let mut covers = Vec::new();
        pick_covers(&page.rows, &coverless, &mut HashSet::new(), &mut covers, 4);
        Ok(Some(Week { name: def.name, source: page.source, total: page.total, genres: page.genres.into_iter().take(4).collect(), played, covers }))
    }

    /// The first `n` records of a crate not saved (the search crate card's, 0.13) in `look`'s order, each by its first
    /// playable track: read a page at a time until they are found, so the card asks only the tracks it shows.
    pub async fn crate_covers(&self, conditions: &[crate::crates::Condition], want: Option<&str>, look: &super::CrateLook, n: usize) -> Result<Vec<WeekCover>, ToggleError> {
        let (source, lines) = self.crates.crate_lines_in_order(conditions, want, look).await?;
        let coverless = coverless(self.served.as_deref()).await;
        let (mut seen, mut covers) = (HashSet::new(), Vec::new());
        for page in lines.chunks(COVER_ROWS).take(COVER_ROWS_AT_MOST / COVER_ROWS) {
            let rows = self.playlists.rows_of(page.to_vec(), &source).await?;
            pick_covers(&rows, &coverless, &mut seen, &mut covers, n);
            if covers.len() >= n {
                break;
            }
        }
        Ok(covers)
    }
}
