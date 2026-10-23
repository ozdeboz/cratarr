//! Playing a collection (the owner, 2026-10-23: "we should limit to maximum 500 tracks in our queue"): one operation
//! for every surface and every player. What plays is the server's choice, from the whole collection, not from whatever
//! the screen pressed had loaded (Unlisted's 10,000 tracks and more played the same few, its panel's first page).
//!
//! - **What:** a crate (its rules, its two toggles as the page shows them; a genre is the crate `genre:<name>`), a
//!   playlist, an album, an artist, or tracks named one by one (a selection, a queue).
//! - **How:** Play (the collection's order, its first [`QUEUE_MOST`], or from a given track on), Shuffle
//!   ([`QUEUE_MOST`] drawn evenly from all of it, then mixed; all of it when it is that small), Play next and Add to
//!   queue (in order, as Play takes them).
//! - **Where:** the answer always carries the tracks (the page's own player queues them itself); asked, the server
//!   also queues them on the server's player (as `/api/queue/add` and a playlist's Play reach it) or on one of
//!   Cratarr's players by name (Play On, decision 0031).

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{App, QueueTrack, ToggleError};
use crate::crates::Favourites;
use crate::source::{self, PlayHow};

/// The most tracks one play puts in a queue (the owner, 2026-10-23).
pub const QUEUE_MOST: usize = 500;

/// What is played.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Collection {
    /// A crate, a genre's too (`genre:<name>`); its toggles as the page shows them (a tap may be on its way), else as
    /// saved.
    Crate {
        id: String,
        #[serde(default)]
        favourites: Option<Favourites>,
        #[serde(default)]
        favourite_albums: Option<Favourites>,
    },
    Playlist {
        id: String,
    },
    Album {
        source: String,
        id: String,
    },
    Artist {
        source: String,
        name: String,
    },
    Tracks {
        source: String,
        ids: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum How {
    Play,
    Shuffle,
    Next,
    Last,
}

/// Where the server queues them itself: the server's player (the one that plays, or the one followed), or one of
/// Cratarr's players by its name.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum On {
    Server,
    Player(String),
}

#[derive(Debug, Deserialize)]
pub struct Request {
    pub what: Collection,
    pub how: How,
    /// From this track (its id): the collection's order from it on, or shuffled with it first.
    #[serde(default)]
    pub from: Option<String>,
    /// Left out: only the answer (the asker plays the tracks itself).
    #[serde(default)]
    pub on: Option<On>,
    /// The queue's name on a player of Cratarr's; else the collection's.
    #[serde(default)]
    pub name: Option<String>,
}

/// The tracks chosen, in queue order, as players hold them (`n`: each one's place in the collection, so turning shuffle
/// off puts them in its order), the one that plays first, how many the collection has that can be played, and the
/// player that took them when the server queued them.
#[derive(Debug, Serialize)]
pub struct Played {
    pub source: String,
    pub tracks: Vec<QueueTrack>,
    pub index: usize,
    pub total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player: Option<String>,
}

/// A small generator (xorshift), seeded from the operating system: an even draw needs no better.
pub struct Rng(u64);

impl Rng {
    pub fn new() -> Self {
        let mut b = [0u8; 8];
        let seed = match getrandom::fill(&mut b) {
            Ok(()) => u64::from_le_bytes(b),
            Err(_) => std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1),
        };
        Self(seed | 1)
    }

    #[cfg(test)]
    fn seeded(n: u64) -> Self {
        Self(n | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// From 0 up to (not including) `n`.
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// `k` of `pool`, each as likely as any other, in a random order (the first `k` steps of Fisher–Yates).
fn draw(mut pool: Vec<usize>, k: usize, rng: &mut Rng) -> Vec<usize> {
    let k = k.min(pool.len());
    for i in 0..k {
        let j = i + rng.below(pool.len() - i);
        pool.swap(i, j);
    }
    pool.truncate(k);
    pool
}

/// Which of a collection's `n` playable tracks go in the queue (their places in it), in queue order, and the place in
/// the queue that plays first. `at`: the track asked to play first.
///
/// - Play, Play next, Add to queue: `most` in the collection's order. From `at` on; when fewer than `most` follow it,
///   those before it fill up the queue, so a collection that small is queued whole from that track.
/// - Shuffle: `most` drawn evenly from all `n`, in a random order; `at` first, when given.
pub fn pick(n: usize, how: How, at: Option<usize>, most: usize, rng: &mut Rng) -> (Vec<usize>, usize) {
    let at = at.filter(|&a| a < n);
    let most = most.max(1);
    match how {
        How::Shuffle => match at {
            Some(a) => {
                let rest = draw((0..n).filter(|&i| i != a).collect(), most - 1, rng);
                (std::iter::once(a).chain(rest).collect(), 0)
            }
            None => (draw((0..n).collect(), most, rng), 0),
        },
        How::Play | How::Next | How::Last => {
            let a = at.unwrap_or(0);
            let start = a.min(n.saturating_sub(most));
            ((start..n.min(start + most)).collect(), a - start)
        }
    }
}

/// A collection's playable tracks, in its order: the server their ids are on, and each one's id and, for lines of a
/// playlist or a crate, its line (its name is read only for the tracks chosen).
struct Candidates {
    source: String,
    ids: Vec<String>,
    lines: Option<Vec<String>>,
    /// Tracks that come with their names (an album's, an artist's).
    named: Option<Vec<QueueTrack>>,
    /// The collection's own name, for a queue on a player of Cratarr's.
    name: Option<String>,
}

/// Plays a collection (see the module's notes). `want`: the server to use (the page's), else the first connected.
pub async fn play(app: &Arc<App>, want: Option<&str>, r: Request) -> Result<Played, ToggleError> {
    let c = candidates(app, want, &r.what).await?;
    let total = c.ids.len();
    if total == 0 {
        return Err(ToggleError::CannotPlay("Nothing in it can be played.".into()));
    }
    let at = r.from.as_deref().and_then(|f| c.ids.iter().position(|id| id == f));
    let (chosen, index) = pick(total, r.how, at, QUEUE_MOST, &mut Rng::new());
    let (tracks, index) = tracks_of(app, &c, &chosen, index).await?;
    if tracks.is_empty() {
        return Err(ToggleError::CannotPlay("Nothing in it can be played.".into()));
    }
    let player = match r.on {
        None => None,
        Some(On::Server) => Some(on_server(app, &c.source, &tracks, index, r.how).await?),
        Some(On::Player(name)) => {
            let queue_name = r.name.or(c.name);
            on_player(app, &name, tracks.clone(), index, r.how, queue_name).await?;
            Some(name)
        }
    };
    Ok(Played { source: c.source, tracks, index, total, player })
}

async fn candidates(app: &Arc<App>, want: Option<&str>, what: &Collection) -> Result<Candidates, ToggleError> {
    let cat = app.library.catalogue();
    match what {
        Collection::Crate { id, favourites, favourite_albums } => {
            let name = app.playlists.crate_def(id).await?.name;
            let shown = app.playlists.crate_shown(id, *favourites, *favourite_albums).await?;
            let (source, lines) = app.playlists.crate_lines_of(&shown, want).await?;
            let sid = source.clone();
            // A crate's lines are Music's files as the catalogue has them: there is no missing one to leave out.
            let (ids, lines): (Vec<String>, Vec<String>) = blocking(move || {
                let found = cat.line_track_ids(&sid, &lines)?;
                Ok(lines.into_iter().zip(found).filter_map(|(l, id)| Some((id?, l))).unzip())
            })
            .await?;
            Ok(Candidates { source, ids, lines: Some(lines), named: None, name: Some(name) })
        }
        Collection::Playlist { id } => {
            let r = app.playlists.reach(id)?;
            let entries = r.store.entries(&r.name).await?;
            let (source, _) = app.playlists.source_of(want)?;
            let (sid, music) = (source.clone(), app.library.music().map(std::path::PathBuf::from));
            // As its view plays it: the lines whose file is there and known to the server.
            let (ids, lines): (Vec<String>, Vec<String>) = blocking(move || {
                let found = cat.line_track_ids(&sid, &entries)?;
                Ok(entries
                    .into_iter()
                    .zip(found)
                    .filter(|(l, _)| music.as_deref().is_some_and(|d| crate::playlists::entry_exists(d, l)))
                    .filter_map(|(l, id)| Some((id?, l)))
                    .unzip())
            })
            .await?;
            Ok(Candidates { source, ids, lines: Some(lines), named: None, name: Some(id.clone()) })
        }
        Collection::Album { source, id } => {
            let v = album_view(app, source, id).await?;
            let album = v.id.clone();
            let named: Vec<QueueTrack> = v
                .rows
                .into_iter()
                .filter(|r| !r.missing)
                .filter_map(|r| {
                    Some(QueueTrack {
                        source: v.source.clone(),
                        id: r.id?,
                        name: r.name.unwrap_or(r.file),
                        artist: r.artist,
                        album: r.album,
                        album_id: Some(album.clone()),
                        duration_secs: r.duration_secs,
                        ..QueueTrack::default()
                    })
                })
                .collect();
            Ok(Candidates { source: v.source, ids: named.iter().map(|t| t.id.clone()).collect(), lines: None, named: Some(named), name: Some(v.title) })
        }
        Collection::Artist { source, name } => {
            let v = app.playing.artist_view(source, name).await?;
            let named: Vec<QueueTrack> = v
                .rows
                .into_iter()
                .filter(|r| !r.missing)
                .filter_map(|r| {
                    Some(QueueTrack {
                        source: v.source.clone(),
                        id: r.id?,
                        name: r.name.unwrap_or(r.file),
                        artist: r.artist,
                        album: r.album,
                        album_id: r.album_id,
                        duration_secs: r.duration_secs,
                        ..QueueTrack::default()
                    })
                })
                .collect();
            Ok(Candidates { source: v.source, ids: named.iter().map(|t| t.id.clone()).collect(), lines: None, named: Some(named), name: Some(v.name) })
        }
        Collection::Tracks { source, ids } => {
            let ids: Vec<String> = ids.iter().filter(|i| !i.is_empty()).cloned().collect();
            Ok(Candidates { source: source.clone(), ids, lines: None, named: None, name: None })
        }
    }
}

/// An album's view, following one that left Incoming to where it is now (as the album panel does).
async fn album_view(app: &Arc<App>, source: &str, id: &str) -> Result<super::AlbumView, ToggleError> {
    let failed = match app.playing.album_view(source, id).await {
        Ok(v) if !v.rows.is_empty() => return Ok(v),
        Ok(v) => Ok(v),
        Err(e) => Err(e),
    };
    match app.incoming.album_fate(source, id).await {
        Some(super::AlbumFate::Moved(now)) if now != id => app.playing.album_view(source, &now).await,
        _ => failed,
    }
}

/// The chosen tracks with their names, and the first one's place among them (a track that cannot be named any more is
/// left out).
async fn tracks_of(app: &Arc<App>, c: &Candidates, chosen: &[usize], index: usize) -> Result<(Vec<QueueTrack>, usize), ToggleError> {
    let numbered = |mut t: QueueTrack, at: usize| {
        t.n = u32::try_from(at).ok();
        t
    };
    let found: Vec<Option<QueueTrack>> = if let Some(named) = &c.named {
        chosen.iter().map(|&i| Some(numbered(named[i].clone(), i))).collect()
    } else if let Some(lines) = &c.lines {
        let rows = app.playlists.rows_of(chosen.iter().map(|&i| lines[i].clone()).collect(), &c.source).await?;
        rows.into_iter()
            .zip(chosen)
            .map(|(r, &i)| {
                let id = r.id.filter(|id| *id == c.ids[i])?;
                Some(numbered(
                    QueueTrack {
                        source: c.source.clone(),
                        id,
                        name: r.name.unwrap_or(r.file),
                        artist: r.artist,
                        album: r.album,
                        album_id: r.album_id,
                        duration_secs: r.duration_secs,
                        ..QueueTrack::default()
                    },
                    i,
                ))
            })
            .collect()
    } else {
        // Named by their ids: what the catalogue knows of them (a track outside it plays without; the player asks).
        let (cat, sid, ids) = (app.library.catalogue(), c.source.clone(), chosen.iter().map(|&i| c.ids[i].clone()).collect::<Vec<_>>());
        let rows = blocking(move || cat.source_rows(&sid, &ids)).await.unwrap_or_default();
        chosen
            .iter()
            .enumerate()
            .map(|(k, &i)| {
                let r = rows.get(k);
                Some(numbered(
                    QueueTrack {
                        source: c.source.clone(),
                        id: c.ids[i].clone(),
                        name: r.and_then(|r| r.title.clone()).unwrap_or_default(),
                        artist: r.and_then(|r| r.artist.clone()),
                        album: r.and_then(|r| r.album.clone()),
                        duration_secs: r.and_then(|r| r.duration_ms).map(|ms| ms as f64 / 1000.0),
                        ..QueueTrack::default()
                    },
                    i,
                ))
            })
            .collect()
    };
    let index = found[..index.min(found.len())].iter().filter(|t| t.is_some()).count();
    let tracks: Vec<QueueTrack> = found.into_iter().flatten().collect();
    let index = index.min(tracks.len().saturating_sub(1));
    Ok((tracks, index))
}

/// Queues them on the server's player: Play and Shuffle replace what it plays (from the first to play), Play next and
/// Add to queue go to the player that plays. A Jellyfin player (Finamp) holds fewer than a queue here: it gets what it
/// holds.
async fn on_server(app: &Arc<App>, source_id: &str, tracks: &[QueueTrack], index: usize, how: How) -> Result<String, ToggleError> {
    let (active, s) = app
        .library
        .connected()
        .into_iter()
        .find(|(a, _)| a.cfg.id == source_id)
        .ok_or_else(|| ToggleError::CannotPlay("That server is not connected.".into()))?;
    let most = if active.cfg.kind == source::jellyfin::KIND.id { super::playlists::PLAY_MAX } else { QUEUE_MOST };
    let from = if matches!(how, How::Play | How::Shuffle) { index } else { 0 };
    let ids: Vec<String> = tracks[from..].iter().take(most).map(|t| t.id.clone()).collect();
    match how {
        How::Play | How::Shuffle => s.play_tracks(&ids, PlayHow::Now).await.map_err(|e| ToggleError::CannotPlay(format!("{e:#}"))),
        How::Next => app.playing.queue_add(Some(source_id), &ids, PlayHow::Next).await,
        How::Last => app.playing.queue_add(Some(source_id), &ids, PlayHow::Last).await,
    }
}

/// Queues them on one of Cratarr's players (Play On): a new queue under the collection's name, or added by hand.
async fn on_player(app: &Arc<App>, name: &str, tracks: Vec<QueueTrack>, index: usize, how: How, queue_name: Option<String>) -> Result<(), ToggleError> {
    use super::PlayOnCommand as Command;
    let command = match how {
        How::Play | How::Shuffle => Command::Play {
            from: Default::default(),
            tracks: Some(tracks),
            index,
            position_ms: 0,
            shuffle: false,
            paused: false,
            queue_id: None,
            queue_name,
        },
        How::Next | How::Last => Command::Add {
            from: Default::default(),
            tracks,
            at: None,
            after: None,
            how: Some(if how == How::Next { "next" } else { "last" }.into()),
        },
    };
    app.play_on.command(name, command).await
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> Result<T, ToggleError> {
    Ok(tokio::task::spawn_blocking(f).await.map_err(|e| anyhow::anyhow!("{e}"))??)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn distinct(v: &[usize]) -> bool {
        let mut s = v.to_vec();
        s.sort_unstable();
        s.windows(2).all(|w| w[0] != w[1])
    }

    #[test]
    fn a_shuffle_draws_evenly_from_the_whole_of_a_big_collection() {
        // Unlisted's size: every part of it is as likely as the first page.
        let (n, rounds) = (10_000, 400);
        let mut rng = Rng::seeded(7);
        let mut tenths = [0usize; 10];
        let mut first_page = 0;
        for _ in 0..rounds {
            let (chosen, index) = pick(n, How::Shuffle, None, QUEUE_MOST, &mut rng);
            assert_eq!((chosen.len(), index), (QUEUE_MOST, 0));
            assert!(distinct(&chosen) && chosen.iter().all(|&i| i < n));
            for &i in &chosen {
                tenths[i * 10 / n] += 1;
                first_page += usize::from(i < 200);
            }
        }
        // 200,000 picks: 20,000 a tenth; 4,000 on the first page (2 %). Within a few percent.
        let each = rounds * QUEUE_MOST / 10;
        assert!(tenths.iter().all(|&t| t.abs_diff(each) < each / 20), "{tenths:?}");
        let page = rounds * QUEUE_MOST * 200 / n;
        assert!(first_page.abs_diff(page) < page / 10, "{first_page} of {page}");
    }

    #[test]
    fn a_shuffle_is_mixed_not_in_the_collections_order() {
        let (chosen, _) = pick(10_000, How::Shuffle, None, QUEUE_MOST, &mut Rng::seeded(3));
        let rising = chosen.windows(2).filter(|w| w[0] < w[1]).count();
        assert!((150..350).contains(&rising), "{rising} of 499 rising");
    }

    #[test]
    fn a_small_collection_shuffles_whole() {
        let (mut chosen, index) = pick(300, How::Shuffle, None, QUEUE_MOST, &mut Rng::seeded(1));
        assert_eq!(index, 0);
        chosen.sort_unstable();
        assert_eq!(chosen, (0..300).collect::<Vec<_>>());
        assert_eq!(pick(0, How::Shuffle, None, QUEUE_MOST, &mut Rng::seeded(1)).0, Vec::<usize>::new());
    }

    #[test]
    fn a_shuffle_from_a_track_plays_it_first_and_draws_the_rest() {
        let (chosen, index) = pick(10_000, How::Shuffle, Some(9_876), QUEUE_MOST, &mut Rng::seeded(5));
        assert_eq!((chosen.len(), chosen[0], index), (QUEUE_MOST, 9_876, 0));
        assert!(distinct(&chosen));
        let (chosen, _) = pick(3, How::Shuffle, Some(1), QUEUE_MOST, &mut Rng::seeded(5));
        assert_eq!((chosen.len(), chosen[0]), (3, 1));
    }

    #[test]
    fn play_takes_the_first_ones_in_order_at_most_the_cap() {
        let mut rng = Rng::seeded(1);
        assert_eq!(pick(10_000, How::Play, None, QUEUE_MOST, &mut rng), ((0..500).collect(), 0));
        assert_eq!(pick(42, How::Play, None, QUEUE_MOST, &mut rng), ((0..42).collect(), 0));
        assert_eq!(pick(10_000, How::Next, None, QUEUE_MOST, &mut rng), ((0..500).collect(), 0));
        assert_eq!(pick(10_000, How::Last, None, QUEUE_MOST, &mut rng).0.len(), QUEUE_MOST);
    }

    #[test]
    fn play_from_a_track_queues_from_it_and_fills_up_with_those_before() {
        let mut rng = Rng::seeded(1);
        // Far from the end: from it on.
        assert_eq!(pick(1_000, How::Play, Some(300), QUEUE_MOST, &mut rng), ((300..800).collect(), 0));
        // Near the end: those before it fill up the queue, and it plays first.
        assert_eq!(pick(1_000, How::Play, Some(900), QUEUE_MOST, &mut rng), ((500..1_000).collect(), 400));
        // A small collection: whole, from that track.
        assert_eq!(pick(50, How::Play, Some(20), QUEUE_MOST, &mut rng), ((0..50).collect(), 20));
        // A track not in it: from the start.
        assert_eq!(pick(50, How::Play, Some(99), QUEUE_MOST, &mut rng), ((0..50).collect(), 0));
    }

    #[test]
    fn the_request_names_each_kind_of_collection() {
        let r: Request = serde_json::from_value(serde_json::json!({
            "what": { "kind": "crate", "id": "unlisted", "favourites": "only" }, "how": "shuffle", "on": "server"
        }))
        .unwrap();
        assert_eq!(r.what, Collection::Crate { id: "unlisted".into(), favourites: Some(Favourites::Only), favourite_albums: None });
        assert_eq!((r.how, r.on), (How::Shuffle, Some(On::Server)));
        let r: Request = serde_json::from_value(serde_json::json!({
            "what": { "kind": "tracks", "source": "cratarr", "ids": ["tr-1", "tr-2"] }, "how": "next", "on": { "player": "Lenovo Tab" }
        }))
        .unwrap();
        assert_eq!(r.what, Collection::Tracks { source: "cratarr".into(), ids: vec!["tr-1".into(), "tr-2".into()] });
        assert_eq!(r.on, Some(On::Player("Lenovo Tab".into())));
        for what in [
            serde_json::json!({ "kind": "playlist", "id": "Chill" }),
            serde_json::json!({ "kind": "album", "source": "cratarr", "id": "al-1" }),
            serde_json::json!({ "kind": "artist", "source": "cratarr", "name": "Air" }),
            serde_json::json!({ "kind": "crate", "id": "genre:Jazz" }),
        ] {
            let r: Request = serde_json::from_value(serde_json::json!({ "what": what, "how": "play" })).unwrap();
            assert_eq!((r.on, r.from), (None, None));
        }
    }
}
