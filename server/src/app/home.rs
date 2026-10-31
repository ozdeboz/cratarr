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
//!
//! Jump back in is built once for those asking at once and kept 20 seconds for each asker (`BackKeep`), and a play
//! reported builds it again at the next ask (0.12.3: Home's Jump back in asked dozens of times a minute, 2026-10-31).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
/// How long one asker's Jump back in is kept (0.12.3: Home's Jump back in asked dozens of times a minute, 2026-10-31:
/// up to 59 a minute, two pages often in the same second, each build 1.3 to 3 seconds and the memory 256 to 408 MB).
/// What it shows changes as albums are played, minutes apart.
const BACK_KEEP: Duration = Duration::from_secs(20);

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

/// Each server's listed albums as the Library's one list has them (`App::albums`), from Music's lists (`music`) and the
/// other libraries' (`other`): the servers of Music's lists, then those of the others' alone; a server's Music albums,
/// then its other libraries'. Only what Jump back in reads of that list, without what it adds (classical facts,
/// verdicts, what may be deleted) or a copy of every album (0.12.3: Home's Jump back in asked dozens of times a
/// minute, 2026-10-31).
fn listed<'a>(music: &[(&'a str, &'a [source::Album])], other: &[(&'a str, &'a [source::Album])]) -> Vec<(&'a str, Vec<&'a source::Album>)> {
    let mut ids: Vec<&'a str> = music.iter().map(|(id, _)| *id).collect();
    for (id, _) in other {
        if !ids.contains(id) {
            ids.push(*id);
        }
    }
    ids.into_iter()
        .map(|id| {
            let m: &'a [source::Album] = music.iter().find(|(s, _)| *s == id).map(|(_, a)| *a).unwrap_or_default();
            let o: &'a [source::Album] = other.iter().find(|(s, _)| *s == id).map(|(_, a)| *a).unwrap_or_default();
            (id, m.iter().chain(o.iter()).collect())
        })
        .collect()
}

/// Who Jump back in is built for: the asking account, whether it asks from away from home (the players it reaches,
/// `PlayOn::left_queues`), and the asking player.
type BackKey = (Option<String>, bool, Option<String>);

/// One asker's Jump back in as last built, and when.
type BackSlot = tokio::sync::Mutex<Option<(Instant, Arc<Vec<Back>>)>>;

/// Jump back in, built once for those asking at once and kept a while for each asker (0.12.3: Home's Jump back in
/// asked dozens of times a minute, 2026-10-31), as `import::Matcher::library_albums` keeps the library's albums.
struct BackKeep {
    keep: Duration,
    slots: std::sync::Mutex<BTreeMap<BackKey, Arc<BackSlot>>>,
}

static BACK: BackKeep = BackKeep::new(BACK_KEEP);

impl BackKeep {
    const fn new(keep: Duration) -> Self {
        Self { keep, slots: std::sync::Mutex::new(BTreeMap::new()) }
    }

    /// `key`'s answer while it is young enough, else `build`'s, kept. The asker's slot is held while building, so those
    /// asking the same meanwhile wait and share the one answer; other askers build alongside. One that fails is not
    /// kept.
    async fn kept_or_built<F, Fut>(&self, key: BackKey, build: F) -> Result<Arc<Vec<Back>>, ToggleError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Vec<Back>, ToggleError>>,
    {
        let slot = {
            let mut slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
            // Those gone stale and not in use go: an account's players come and go.
            slots.retain(|k, s| {
                *k == key || Arc::strong_count(s) > 1 || s.try_lock().is_ok_and(|kept| matches!(&*kept, Some((at, _)) if at.elapsed() < self.keep))
            });
            slots.entry(key).or_insert_with(|| Arc::new(tokio::sync::Mutex::new(None))).clone()
        };
        let mut kept = slot.lock().await;
        if let Some((at, answer)) = &*kept
            && at.elapsed() < self.keep
        {
            return Ok(answer.clone());
        }
        *kept = None;
        let answer = Arc::new(build().await?);
        *kept = Some((Instant::now(), answer.clone()));
        Ok(answer)
    }

    /// `account`'s answers built again at their next ask (a play of theirs reported).
    fn forget(&self, account: Option<&str>) {
        self.slots.lock().unwrap_or_else(|p| p.into_inner()).retain(|(a, _, _), _| a.as_deref() != account);
    }
}

/// The running request's asker of Jump back in (`BackKey`).
fn back_key(served: Option<&crate::serve::Serve>, player: Option<&str>) -> BackKey {
    let away = crate::serve::who::current().is_some_and(|w| !w.home);
    (served.and_then(|s| s.current_user()), away, player.map(str::to_string))
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

    /// Jump back in reads the Library's one list's servers and albums in that list's order (`App::albums`): Music's
    /// servers, then the other libraries' alone; a server's Music albums, then its others'.
    #[test]
    fn jump_back_in_reads_the_servers_and_albums_in_the_librarys_order() {
        let card = |id: &str| crate::source::cratarr::card(album(id, 3));
        let (own_music, jf_music, own_other, more_other) = ([card("m1"), card("m2")], [card("j1")], [card("i1")], [card("o1")]);
        let music = [("cratarr", &own_music[..]), ("jellyfin", &jf_music[..])];
        let other = [("other", &more_other[..]), ("cratarr", &own_other[..])];
        let got: Vec<(&str, Vec<&str>)> = listed(&music, &other).into_iter().map(|(s, l)| (s, l.into_iter().map(|a| a.id.as_str()).collect())).collect();
        assert_eq!(got, [("cratarr", vec!["m1", "m2", "i1"]), ("jellyfin", vec!["j1"]), ("other", vec!["o1"])]);
    }

    fn back(id: &str) -> Back {
        Back::Album { at: 1, source: "cratarr".into(), id: id.into(), title: id.into(), by: None }
    }

    fn asker(account: &str, player: &str) -> BackKey {
        (Some(account.into()), false, Some(player.into()))
    }

    /// Jump back in is built once for those asking at once and shared; again for another player or account, after a
    /// play of the account's, and once its keep has run out; one that failed is not kept (0.12.3: Home's Jump back in
    /// asked dozens of times a minute, 2026-10-31).
    #[tokio::test]
    async fn those_asking_jump_back_in_at_once_share_one_build_kept_a_while() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let keep = &BackKeep::new(Duration::from_millis(400));
        let builds = &AtomicUsize::new(0);
        let build = move || async move {
            builds.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok::<_, ToggleError>(vec![back("x")])
        };
        let ask = move |account: &str, player: &str| keep.kept_or_built(asker(account, player), build);
        let built = || builds.load(Ordering::SeqCst);
        // Two pages in the same second, twice over: one build, the same answer for all.
        let (a, b, c, d) = tokio::join!(ask("ann", "web"), ask("ann", "web"), ask("ann", "web"), ask("ann", "web"));
        let (a, b, c, d) = (a.unwrap(), b.unwrap(), c.unwrap(), d.unwrap());
        assert_eq!(built(), 1);
        for x in [&b, &c, &d] {
            assert!(Arc::ptr_eq(&a, x));
        }
        assert_eq!(a.len(), 1);
        // Another player, another account: their own builds.
        ask("ann", "phone").await.unwrap();
        ask("bob", "web").await.unwrap();
        assert_eq!(built(), 3);
        // The first again within its keep: kept.
        assert!(Arc::ptr_eq(&ask("ann", "web").await.unwrap(), &a));
        assert_eq!(built(), 3);
        // A play of ann's reported: hers built again, bob's kept.
        keep.forget(Some("ann"));
        assert!(!Arc::ptr_eq(&ask("ann", "web").await.unwrap(), &a));
        ask("bob", "web").await.unwrap();
        assert_eq!(built(), 4);
        // Its keep run out: built again.
        tokio::time::sleep(Duration::from_millis(450)).await;
        ask("bob", "web").await.unwrap();
        assert_eq!(built(), 5);
        // One that failed is not kept.
        let failing = || async { Err::<Vec<Back>, _>(ToggleError::Refused("away".into())) };
        assert!(keep.kept_or_built(asker("cy", "web"), failing).await.is_err());
        ask("cy", "web").await.unwrap();
        assert_eq!(built(), 6);
    }
}
