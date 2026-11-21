//! Radio and Autoplay (decision 0036, amended): the music goes on from the owner's own library. The player that plays
//! asks Cratarr (`POST /api/radio/next`) and fills two lanes: Radio's inserts, about five tracks like each of the
//! queue's own tracks right after it (`mode: "radio"`), and Autoplay's tail, appended when fewer than three tracks are
//! left (`mode: similar | artist | era | random | taste`, `albums` for whole albums). Cratarr chooses; the player keeps
//! both switches and fills its lanes. Your taste follows the listener's taste, and in every mode what is like their
//! skips comes later and less often (decision 0094).
//!
//! Cratarr's own library only. The index below reads the library and the user data from their own SQLite files, read
//! only, and the playlists from the playlist service; it is built again when the library is, and at least every ten
//! minutes (playlists and album tags change in between). The listener's favourites and plays are read for each choice:
//! the account whose player asks (decision 0051).
//!
//! The picker lives in the rules crate (`cratarr_rules::radio`), and is named here too, so the server says `radio::Mode`
//! as it always did. What stays is the server's own: the index read from its files, the listener's marks, Bandcamp.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
#[cfg(test)]
use rusqlite::Connection;
use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::ToggleError;
#[cfg(test)]
use super::play::Rng;
use super::playing_host::PlayingHost;
use super::playon::QueueTrack;
use crate::similarity::{Signal, Track, Weights, unique};

pub use cratarr_rules::radio::{Ask, Index, MAX_COUNT, Mode, RadioBy, Song, choose};
pub use super::radio_base::{AutoMode, AutoplayMode, Within};

/// The lane of Radio's inserts (`QueueTrack::lane`): right after the own track they are like (and its hand-added run).
pub const LANE_RADIO: &str = "radio";
/// The lane of Autoplay's tracks: after everything, when the queue runs out.
pub const LANE_AUTOPLAY: &str = "autoplay";

/// A radio mode from before the amendment, as Autoplay: (mode, albums, restart).
pub fn from_old(mode: Option<Mode>) -> (AutoMode, bool, bool) {
    match mode {
        Some(Mode::Artist) => (AutoMode::Artist, false, false),
        Some(Mode::Era) => (AutoMode::Era, false, false),
        Some(Mode::Random) => (AutoMode::Random, false, false),
        Some(Mode::AlbumMix) => (AutoMode::Similar, true, false),
        Some(Mode::Reshuffle) => (AutoMode::Similar, false, true),
        _ => (AutoMode::Similar, false, false),
    }
}

/// How many tracks Radio inserts after each own track, unless set; and the most it inserts.
pub const RADIO_COUNT: u8 = 5;
pub const RADIO_MAX: u8 = 10;

/// Autoplay as the queue keeps it (`Queue::autoplay`): on while there is one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Autoplay {
    #[serde(default)]
    pub mode: AutoMode,
    /// Whole albums instead of tracks, while albums were playing (was Album mix).
    #[serde(default)]
    pub albums: bool,
    /// A playlist that ends starts again, shuffled if it played shuffled, instead of the mode (was Reshuffle). The
    /// player does it.
    #[serde(default)]
    pub restart: bool,
    /// The tracks it stays close to (ids), when started from an item ("Start radio"); empty: the queue's own tracks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seed: Vec<String>,
    /// What it plays inside, only its tracks, endlessly ("Play as radio"; decision 0094: a crate, a playlist, a record,
    /// an artist or tracks).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub within: Option<Within>,
}

/// `Within` as a play names it: here, as `Within` sits in base, below the play it names.
pub trait AsCollection {
    fn collection(&self) -> super::play::Collection;
}

impl AsCollection for Within {
    /// As a play names it (app::play), to read its tracks as a play does.
    fn collection(&self) -> super::play::Collection {
        use super::play::Collection;
        match self.clone() {
            Within::Crate { id, without } => {
                Collection::Crate { id, favourites: None, favourite_albums: None, without, filter: Vec::new(), order: super::CrateOrder::Own, down: false }
            }
            Within::Playlist { id } => Collection::Playlist { id },
            Within::Album { source, id } => Collection::Album { source, id },
            Within::Artist { source, name, id, without } => Collection::Artist { source, name, id, without },
            Within::Tracks { source, ids } => Collection::Tracks { source, ids },
        }
    }
}

/// What a player asks for: what the picker takes, beside what only the server can answer.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NextRequest {
    #[serde(flatten)]
    pub ask: Ask,
    /// What to stay inside (decision 0094: a crate, a playlist, a record, an artist or tracks): only its tracks of
    /// Cratarr's own library; endless, a new round once all have played. None: beyond it.
    #[serde(default)]
    pub within: Option<Within>,
    /// Whose More music choice applies (0.9.5): a player Cratarr drives passes its account; left out, the listener's.
    #[serde(skip)]
    pub account: Option<String>,
}

/// The listener's own "Similar by" ([similar_by_of] the account asking).
fn similar_by_of_listener() -> Option<Vec<Signal>> {
    similar_by_of(&crate::serve::instance()?.current_user()?)
}

/// An account's Similar by and More music choice, and what Radio leaves for later: the crates read them too, so they sit
/// below playing (`crates_from_radio`).
pub use super::crates_from_radio::{more_music_of, similar_by_of};
use super::crates_from_radio::{read_only, recent_plays};
#[cfg(test)]
use super::crates_from_radio::{more_music_in, similar_by_in};

/// An index older than this is built again even when the library is not.
const FRESH: Duration = Duration::from_secs(600);

// --- the index, from the library's and the user data's own files ---

struct Cache {
    index: Arc<Index>,
    built_from: String,
    at: Instant,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// The album artist Radio takes a song's artists to go on with: in `serve::index_rows`, below the app, which reads it too.
pub(crate) use crate::serve::index_rows::ALBUM_ARTIST_HEARD;

/// A request as its listener's settings make it: their Similar by, and the libraries they let Radio and Autoplay choose
/// from (0.9.5: their folders). With the account it is for, and whether Bandcamp joins.
pub fn as_asked(mut req: NextRequest) -> Result<(NextRequest, Option<String>, bool), ToggleError> {
    if req.ask.similar_by.is_none() {
        req.ask.similar_by = similar_by_of_listener();
    }
    let account = req.account.clone().or_else(|| crate::serve::instance()?.current_user());
    let (libraries, bandcamp) = account.as_deref().map(more_music_of).unwrap_or((None, false));
    if let (Some(libraries), Some(served)) = (libraries, crate::serve::instance()) {
        let folders: Vec<i64> =
            served.library.roots().into_iter().filter(|(_, r)| libraries.iter().any(|l| Path::new(l) == Path::new(r))).map(|(f, _)| f).collect();
        if folders.is_empty() {
            return Err(ToggleError::Refused("No library is switched on for Radio and Autoplay.".into()));
        }
        req.ask.folders = Some(folders);
    }
    Ok((req, account, bandcamp))
}

/// The next radio tracks for a player (decision 0036).
pub async fn next<H: PlayingHost>(app: &Arc<H>, req: NextRequest) -> Result<Vec<QueueTrack>, ToggleError> {
    next_with(&**app, req).await
}

/// `next`, on its host (Play On's Start radio asks it so).
pub async fn next_with(app: &dyn PlayingHost, req: NextRequest) -> Result<Vec<QueueTrack>, ToggleError> {
    let (index, recent) = ready().await?;
    let (req, account, bandcamp) = as_asked(req)?;
    let within = match &req.within {
        Some(w) => Some(index.find(&super::play::own_songs_with(app, &w.collection(), account.as_deref()).await?)),
        None => None,
    };
    if within.as_ref().is_some_and(Vec::is_empty) {
        return Err(ToggleError::Refused("None of its tracks are in Cratarr's own library.".into()));
    }
    let index = with_taste(app, index, account.as_deref(), req.ask.mode == Mode::Taste).await?;
    // Radio's inserts in the radio lane; everything else is Autoplay's.
    let lane = if req.ask.mode == Mode::Radio { LANE_RADIO } else { LANE_AUTOPLAY };
    let picks = choose(&index, &req.ask, &recent, within.as_deref(), &mut super::play::rng());
    let tracks: Vec<QueueTrack> = picks.iter().map(|&i| track(&index.songs[i], lane)).collect();
    // Bandcamp's suggestions join where the account lets them, on likeness alone (0.9.5; not inside a list).
    match account {
        Some(account) if bandcamp && within.is_none() && matches!(req.ask.mode, Mode::Similar | Mode::Radio | Mode::Continuous) => {
            Ok(with_elsewhere(&index, &req.ask, &picks, tracks, &account, lane).await)
        }
        _ => Ok(tracks),
    }
}

/// A Bandcamp suggestion as Radio and Autoplay weigh it (0.9.5, the owner: they join "as they come", on likeness
/// alone): its album's page, and what makes it alike as the library's songs are: its artist and the library's artists
/// Discover found it from, its styles as genres, its year; no sound, no playlists.
struct Elsewhere {
    page: String,
    like: Track,
}

/// Discover's suggestions that play on Bandcamp (the account's last look), those whose album is queued left out.
fn elsewhere_of(index: &Index, found: &[super::discover::Suggestion], artist_ids: &HashMap<String, i64>, exclude: &[String]) -> Vec<Elsewhere> {
    let queued: HashSet<String> = exclude.iter().filter_map(|id| crate::bandcamp::read_id(id).map(|(page, _)| page)).collect();
    found
        .iter()
        .filter_map(|s| {
            let page = s.bandcamp.clone().filter(|p| !queued.contains(p))?;
            let names = std::iter::once(s.artist.as_str())
                .chain(s.alike.iter().map(|a| a.seed.as_str()))
                .chain(s.why.iter().filter_map(|w| w.strip_prefix("Like ")));
            let number = |n: &str| index.artist_number(&crate::serve::ids::artist(*artist_ids.get(&crate::serve::ids::artist_key(n))?));
            let mut artists: Vec<u32> = names.filter_map(number).collect();
            artists.dedup();
            let genres = s.genres.iter().filter_map(|g| index.genre_number(g)).collect();
            let year = s.year.and_then(|y| u32::try_from(y).ok()).filter(|y| *y > 0);
            Some(Elsewhere { page, like: Track { artists, genres, year, ..Track::default() } })
        })
        .collect()
}

/// The places Bandcamp's suggestions take among the picks (0.9.5): each, the most alike first, takes the place of the
/// least alike pick it is more alike than; none where it is not more alike than any. (place, suggestion) pairs.
fn mix_elsewhere(index: &Index, req: &Ask, picks: &[usize], elsewhere: &[Elsewhere]) -> Vec<(usize, usize)> {
    let last = req.last.as_ref().and_then(|id| index.by_id.get(id).copied());
    let base: Vec<usize> = if req.mode == Mode::Continuous { last.into_iter().collect() } else { index.find(&req.seed) };
    let base = if base.is_empty() { last.into_iter().collect() } else { base };
    if base.is_empty() || picks.is_empty() || elsewhere.is_empty() {
        return Vec::new();
    }
    let profile = index.profile(&base);
    let weights = req.weights();
    let mut places: Vec<(usize, f64)> = picks.iter().enumerate().map(|(p, &i)| (p, index.score(&profile, i, &weights))).collect();
    // No song of the library at this place: no sound to compare (`usize::MAX`).
    let mut theirs: Vec<(usize, f64)> =
        elsewhere.iter().enumerate().map(|(e, x)| (e, profile.to_track(&index.like, usize::MAX, &x.like).score(&weights))).collect();
    theirs.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut out = Vec::new();
    for (e, score) in theirs {
        places.sort_by(|a, b| a.1.total_cmp(&b.1));
        match places.first() {
            Some(&(place, least)) if score > least => {
                out.push((place, e));
                places.remove(0);
            }
            _ => break,
        }
    }
    out
}

/// The picks with Bandcamp's suggestions in the places they take: a track of each album that streams (any), as a
/// preview (decision 0071: never a play, never scrobbled). An album Bandcamp does not answer for leaves its place.
async fn with_elsewhere(index: &Index, req: &Ask, picks: &[usize], mut tracks: Vec<QueueTrack>, account: &str, lane: &str) -> Vec<QueueTrack> {
    let Some(served) = crate::serve::instance() else { return tracks };
    let (s, a) = (served.clone(), account.to_string());
    let (found, artist_ids) = match crate::run::spawn_blocking(move || anyhow::Ok((super::discover::kept(&s, &a)?, s.artist_ids()?))).await {
        Ok(Ok((Some(f), artist_ids))) => (f.albums, artist_ids),
        _ => return tracks,
    };
    let elsewhere = elsewhere_of(index, &found, &artist_ids, &req.exclude);
    let mut rng = super::play::rng();
    for (place, e) in mix_elsewhere(index, req, picks, &elsewhere) {
        let page = &elsewhere[e].page;
        let album = match crate::bandcamp::Bandcamp::get().album(page).await {
            Ok(album) => album,
            Err(e) => {
                tracing::debug!("radio: {page} not read from Bandcamp: {e:#}");
                continue;
            }
        };
        let streams: Vec<&crate::bandcamp::Track> = album.tracks.iter().filter(|t| t.plays()).collect();
        if streams.is_empty() {
            continue;
        }
        let t = streams[rng.below(streams.len())];
        tracks[place] = QueueTrack {
            lane: Some(lane.into()),
            source: crate::bandcamp::SOURCE.to_string(),
            id: crate::bandcamp::track_id(page, t.n),
            name: t.title.clone(),
            artist: Some(t.artist.clone().unwrap_or_else(|| album.artist.clone())),
            album: Some(album.title.clone()),
            album_id: Some(crate::bandcamp::track_id(page, 0)),
            duration_secs: t.duration_secs,
            ..QueueTrack::default()
        };
    }
    tracks
}

/// The index with the account's taste on its songs (decision 0094): how each fits it (for Your taste only), and how
/// like each is to what the account skips (none while its skipped source is off: `CrateService::skipped_fits`).
pub async fn with_taste(app: &dyn PlayingHost, index: Arc<Index>, account: Option<&str>, taste: bool) -> Result<Arc<Index>, ToggleError> {
    let Some(user) = account else { return Ok(index) };
    let fits = if taste { app.crates().taste_fits(user, &[]).await? } else { HashMap::new() };
    let skipped = app.crates().skipped_fits(user, &[]).await?;
    if fits.is_empty() && skipped.is_empty() {
        return Ok(index);
    }
    let mut theirs = Index::clone(&index);
    for s in &mut theirs.songs {
        s.taste = fits.get(&s.id).copied();
        s.skipped = skipped.get(&s.id).copied().unwrap_or(0.0);
    }
    Ok(Arc::new(theirs))
}

fn track(s: &Song, lane: &str) -> QueueTrack {
    QueueTrack {
        lane: Some(lane.into()),
        source: crate::source::cratarr::KIND.id.to_string(),
        id: s.id.clone(),
        name: s.title.clone(),
        artist: s.artist.clone(),
        album: Some(s.album.clone()).filter(|a| !a.is_empty()),
        album_id: Some(s.album_id.clone()),
        duration_secs: s.duration_secs,
        ..QueueTrack::default()
    }
}

/// The index, built again when the library was or it is ten minutes old.
async fn index(served: &Arc<crate::serve::Serve>, dir: &Path) -> Result<Arc<Index>> {
    let library = dir.join("library.sqlite");
    let built_from = {
        let library = library.clone();
        crate::run::spawn_blocking(move || built_from(&library)).await.map_err(|e| anyhow::anyhow!("{e}"))??
    };
    if let Some(c) = CACHE.lock().unwrap_or_else(|p| p.into_inner()).as_ref()
        && c.built_from == built_from
        && c.at.elapsed() < FRESH
    {
        return Ok(c.index.clone());
    }
    // The playlists' songs: every account's playlists' lines through the catalogue (what people put together).
    let lines = match super::CrateService::held_by(served).map(|c| c.playlists().clone()) {
        Some(p) => p.snapshot_all().await.map_err(|e| anyhow::anyhow!("reading the playlists: {e:?}"))?,
        None => Vec::new(),
    };
    let (served2, dir) = (served.clone(), dir.to_path_buf());
    let index = crate::run::spawn_blocking(move || -> Result<Index> {
        let cat = served2.catalogue();
        let mut member: HashMap<String, Vec<u32>> = HashMap::new();
        for (n, (_, lines, _)) in lines.iter().enumerate() {
            for line in lines {
                if let Some(song) = crate::serve::playlists::song_of_line(cat, line) {
                    member.entry(song).or_default().push(n as u32);
                }
            }
        }
        // Everyone's: the listener's favourites are marked for each choice. Each song's facts as the crates read them
        // (one reading of a track, `Index::with_facts`).
        let facts = cat.crate_facts()?;
        let mut index = read_index(&dir.join("library.sqlite"), &dir.join("userdata.sqlite"), None, &member)?.with_facts(&facts);
        // Each playlist's number is its place in that reading: a playlist's own membership is no signal of it (SIM, Q-SIM-6).
        index.playlist_ids = lines.iter().map(|(id, _, _)| id.clone()).collect();
        Ok(index)
    })
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))??;
    let index = Arc::new(index);
    *CACHE.lock().unwrap_or_else(|p| p.into_inner()) = Some(Cache { index: index.clone(), built_from, at: Instant::now() });
    Ok(index)
}

/// What the library was last built from (its meta row), so an index from before is built again.
fn built_from(library: &Path) -> Result<String> {
    let conn = read_only(library)?;
    Ok(conn.query_row("SELECT value FROM meta WHERE key = 'built_from'", [], |r| r.get(0)).unwrap_or_default())
}

/// The songs `user` hearted.
fn starred_songs(userdata: &Path, user: &str) -> Result<HashSet<String>> {
    let conn = read_only(userdata)?;
    let mut st = conn.prepare("SELECT item FROM annotation WHERE user = ?1 AND kind = 'song' AND starred = 1")?;
    Ok(st.query_map(params![user], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<_>>()?)
}

/// Interns names as numbers: genres and tags in any case, ids exactly.
#[derive(Default)]
struct Names {
    of: HashMap<String, u32>,
    exact: bool,
}

impl Names {
    fn exact() -> Self {
        Self { exact: true, ..Self::default() }
    }
    fn of(&mut self, name: &str) -> u32 {
        let n = self.of.len() as u32;
        let key = if self.exact { name.to_string() } else { name.to_lowercase() };
        *self.of.entry(key).or_insert(n)
    }
    /// The names in their numbers' order.
    fn into_list(self) -> Vec<String> {
        let mut list = vec![String::new(); self.of.len()];
        for (name, n) in self.of {
            list[n as usize] = name;
        }
        list
    }
}

/// The index from the library's file, the user data's file (album tags, favourites) and the playlists' members.
fn read_index(library: &Path, userdata: &Path, owner: Option<&str>, member: &HashMap<String, Vec<u32>>) -> Result<Index> {
    let lib = read_only(library)?;
    let ud = read_only(userdata)?;
    let mut tags_of: HashMap<String, Vec<String>> = HashMap::new();
    {
        let mut st = ud.prepare("SELECT album, tag FROM album_tags")?;
        for r in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (album, tag) = r?;
            tags_of.entry(album).or_default().push(tag);
        }
    }
    let starred = match owner {
        Some(o) => starred_songs(userdata, o)?,
        None => HashSet::new(),
    };
    let mut artists_of: HashMap<String, Vec<String>> = HashMap::new();
    {
        let mut st = lib.prepare("SELECT song_id, artist_id FROM song_artists ORDER BY song_id, position")?;
        for r in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (song, artist) = r?;
            artists_of.entry(song).or_default().push(artist);
        }
    }
    let (mut albums, mut artists, mut genres, mut tags) = (Names::exact(), Names::exact(), Names::default(), Names::default());
    let mut st = lib.prepare(&format!(
        "SELECT s.id, s.title, s.artist, s.album, s.album_id, s.year, s.genre, a.genres, s.duration_ms, s.bpm, {ALBUM_ARTIST_HEARD}, s.folder
         FROM songs s LEFT JOIN albums a ON a.id = s.album_id
         ORDER BY s.album_id, s.disc_no, s.track_no, s.rel"
    ))?;
    let mut songs = Vec::new();
    let rows = st.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, Option<i64>>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, Option<String>>(7)?,
            r.get::<_, Option<i64>>(8)?,
            // The BPM, then the album artist, as the query names them.
            r.get::<_, Option<i64>>(9)?,
            r.get::<_, Option<String>>(10)?,
            r.get::<_, i64>(11)?,
        ))
    })?;
    for r in rows {
        let (id, title, artist, album, album_id, year, genre, album_genres, duration_ms, bpm, album_artist, folder) = r?;
        let mut a: Vec<u32> = artists_of.get(&id).map(|v| v.iter().map(|x| artists.of(x)).collect()).unwrap_or_default();
        a.extend(album_artist.as_deref().map(|x| artists.of(x)));
        let mut g: Vec<u32> = genre.as_deref().filter(|x| !x.is_empty()).map(|x| genres.of(x)).into_iter().collect();
        g.extend(album_genres.as_deref().unwrap_or("").split('\u{1f}').filter(|x| !x.is_empty()).map(|x| genres.of(x)));
        let t: Vec<u32> = tags_of.get(&album_id).map(|v| v.iter().map(|x| tags.of(x)).collect()).unwrap_or_default();
        songs.push(Song {
            favourite: starred.contains(&id),
            excluded: false,
            taste: None,
            skipped: 0.0,
            folders: vec![folder],
            album_n: albums.of(&album_id),
            like: Track {
                playlists: member.get(&id).cloned().unwrap_or_default(),
                artists: a,
                genres: g,
                tags: t,
                year: year.and_then(|y| u32::try_from(y).ok()).filter(|y| *y > 0),
                bpm: bpm.filter(|b| *b > 0).map(|b| b as f32),
                ..Track::default()
            },
            duration_secs: duration_ms.filter(|d| *d > 0).map(|d| d as f64 / 1000.0),
            id,
            title,
            artist,
            album,
            album_id,
        });
    }
    let mut index = Index::new(songs);
    index.artist_ids = artists.into_list();
    index.genre_ids = genres.into_list();
    Ok(index)
}

// --- OpenSubsonic's similar songs and artists, from the same choice (decision 0036) ---

/// Songs like an item (a song, an album or an artist): Similar with the item as its seed, its own songs left out.
pub async fn similar_songs(item: &str, count: usize) -> Result<Vec<String>, ToggleError> {
    let (index, recent) = ready().await?;
    Ok(songs_like(&index, item, count, similar_by_of_listener(), &recent, &mut super::play::rng()))
}

/// `similar_songs`' choice (SIM, Q-SIM-4: similar songs are a radio): as Autoplay's Similar, as many as asked while the library
/// holds them, never dry.
fn songs_like(index: &Index, item: &str, count: usize, similar_by: Option<Vec<Signal>>, recent: &HashMap<String, i64>, rng: &mut cratarr_rules::Rng) -> Vec<String> {
    let seed: Vec<String> = index.of_item(item).into_iter().map(|i| index.songs[i].id.clone()).collect();
    if seed.is_empty() {
        return Vec::new();
    }
    let req = Ask { mode: Mode::Similar, exclude: seed.clone(), seed, last: None, count: Some(count.min(MAX_COUNT)), albums: false, by: RadioBy::Track, skip_favourites: false, similar_by, folders: None };
    choose(index, &req, recent, None, rng).into_iter().map(|i| index.songs[i].id.clone()).collect()
}

/// Lists like a list, of its own kind (the owner, 2026-10-23: "any list that has a profile at the top gets a
/// similarity set at the bottom"): each candidate's profile against the seed's (`similarity::Profile::to_profile`), by
/// the listener's "Similar by" (else all signals alike), the closest first, as 0 to 100. Candidates with none of their
/// songs in the library are left out.
fn closest_lists(index: &Index, seed: &[usize], candidates: Vec<(String, Vec<usize>)>, weights: &Weights, count: usize) -> Vec<(String, f64)> {
    closest_of(index, seed, None, candidates, weights, count)
}

/// `closest_lists` of playlists, `own` the seed's (SIM, Q-SIM-6).
fn closest_playlists(index: &Index, seed: &[usize], own: &str, candidates: Vec<(String, Vec<usize>)>, weights: &Weights, count: usize) -> Vec<(String, f64)> {
    closest_of(index, seed, Some(own), candidates, weights, count)
}

fn closest_of(index: &Index, seed: &[usize], own: Option<&str>, candidates: Vec<(String, Vec<usize>)>, weights: &Weights, count: usize) -> Vec<(String, f64)> {
    // SIM's red commit: today's, a playlist's own membership among its words.
    let _ = own;
    if seed.is_empty() {
        return Vec::new();
    }
    let p = index.profile(seed);
    let mut out: Vec<(String, f64)> = candidates
        .into_iter()
        .filter(|(_, set)| !set.is_empty())
        .map(|(id, set)| {
            let alike = p.to_profile(&index.like, &index.profile(&set)).score(weights);
            (id, (alike * 1000.0).round() / 10.0)
        })
        .collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out.truncate(count);
    out
}

fn listener_weights() -> Weights {
    similar_by_of_listener().map(|o| Weights::ranked(&o)).unwrap_or(Weights::EQUAL)
}

/// Albums like an album (its OpenSubsonic id), the closest first, with how alike (0 to 100); none by its own artist.
pub async fn similar_albums(album: &str, count: usize) -> Result<Vec<(String, f64)>, ToggleError> {
    let (index, _) = ready().await?;
    let (album, weights) = (album.to_string(), listener_weights());
    crate::run::spawn_blocking(move || albums_like(&index, &album, &weights, count)).await.map_err(|e| ToggleError::from(anyhow::anyhow!("{e}")))
}

/// `similar_albums`' choice, by the listener's weights.
fn albums_like(index: &Index, album: &str, weights: &Weights, count: usize) -> Vec<(String, f64)> {
    let seed: Vec<usize> = (0..index.songs.len()).filter(|&i| index.songs[i].album_id == album).collect();
    let own = seed.first().map(|&i| index.songs[i].album_n);
    // An album's own artist: those on every one of its songs (its album artist is among each song's artists). An
    // album by the same artist is no similar album: More from the artist shows those (the owner, 2026-10-25).
    let artists_of = |songs: &[usize]| -> HashSet<u32> {
        let mut each = songs.iter().map(|&i| index.songs[i].like.artists.iter().copied().collect::<HashSet<u32>>());
        let first = each.next().unwrap_or_default();
        each.fold(first, |all, these| &all & &these)
    };
    let by = artists_of(&seed);
    let candidates = index
        .albums
        .iter()
        .filter(|(a, songs)| Some(**a) != own && artists_of(songs).is_disjoint(&by))
        .map(|(_, songs)| (index.songs[songs[0]].album_id.clone(), songs.clone()))
        .collect();
    closest_lists(index, &seed, candidates, weights, count)
}

/// Lists like a list of songs (their OpenSubsonic ids), among `candidates` (an id and its songs' ids), the closest first,
/// with how alike (0 to 100): a playlist's similar playlists (`playlist`: the seed's own id, whose membership is no signal of
/// it), a crate's similar crates.
pub async fn similar_lists(seed: Vec<String>, playlist: Option<String>, candidates: Vec<(String, Vec<String>)>, count: usize) -> Result<Vec<(String, f64)>, ToggleError> {
    let (index, _) = ready().await?;
    let weights = listener_weights();
    crate::run::spawn_blocking(move || {
        let seed = index.find(&seed);
        let candidates = candidates.into_iter().map(|(id, songs)| (id, index.find(&songs))).collect();
        match playlist {
            Some(own) => closest_playlists(&index, &seed, &own, candidates, &weights, count),
            None => closest_lists(&index, &seed, candidates, &weights, count),
        }
    })
    .await
    .map_err(|e| ToggleError::from(anyhow::anyhow!("{e}")))
}

/// Artists like an artist: the artists of the songs Similar finds for it, the most frequent first.
pub async fn similar_artists(artist: &str, count: usize) -> Result<Vec<String>, ToggleError> {
    let (index, recent) = ready().await?;
    Ok(artists_like(&index, artist, similar_by_of_listener(), &recent, &mut super::play::rng(), count))
}

/// `similar_artists`' choice, by the listener's Similar by.
fn artists_like(index: &Index, artist: &str, similar_by: Option<Vec<Signal>>, recent: &HashMap<String, i64>, rng: &mut cratarr_rules::Rng, count: usize) -> Vec<String> {
    let Some(own) = index.artist_ids.iter().position(|x| x == artist).map(|a| a as u32) else { return Vec::new() };
    let seed: Vec<String> = index.of_item(artist).into_iter().map(|i| index.songs[i].id.clone()).collect();
    let req = Ask { mode: Mode::Similar, exclude: seed.clone(), seed, last: None, count: Some(MAX_COUNT), albums: false, by: RadioBy::Track, skip_favourites: false, similar_by, folders: None };
    let mut tally: HashMap<u32, u32> = HashMap::new();
    for i in choose(index, &req, recent, None, rng) {
        for &a in unique(&index.songs[i].like.artists).collect::<Vec<_>>().iter().filter(|&&a| a != own) {
            *tally.entry(a).or_default() += 1;
        }
    }
    let mut ranked: Vec<(u32, u32)> = tally.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    ranked.into_iter().take(count).map(|(a, _)| index.artist_ids[a as usize].clone()).collect()
}

/// The index with the listener's favourites and exclusions marked, and their recent plays, for a choice made here: the
/// running request's account's, whose player asks (decision 0051).
pub async fn ready() -> Result<(Arc<Index>, HashMap<String, i64>), ToggleError> {
    let served = crate::serve::instance().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
    let dir = served.data_dir.clone().ok_or_else(|| ToggleError::Refused("Radio needs Cratarr's data folder.".into()))?;
    let index = index(&served, &dir).await?;
    let user = served.current_user();
    let excluded = match super::CrateService::held_by(&served) {
        Some(p) => p.excluded_songs_of(user.as_deref()).await?,
        None => HashSet::new(),
    };
    let (recent, starred) = match user {
        Some(user) => crate::run::spawn_blocking(move || -> Result<_> {
            let userdata = dir.join("userdata.sqlite");
            // Its play history off (0.9.5): what it played is not learned from.
            let keeps = crate::serve::instance().is_none_or(|s| s.userdata.tracking(&user));
            let recent = if keeps { recent_plays(&userdata, &user)? } else { HashMap::new() };
            Ok((recent, starred_songs(&userdata, &user)?))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))??,
        None => (HashMap::new(), HashSet::new()),
    };
    if starred.is_empty() && excluded.is_empty() {
        return Ok((index, recent));
    }
    let mut theirs = Index::clone(&index);
    for s in &mut theirs.songs {
        s.favourite = starred.contains(&s.id);
        s.excluded = excluded.contains(&s.id);
    }
    Ok((Arc::new(theirs), recent))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_radios_recent_plays_are_ones_own() {
        let dir = std::env::temp_dir().join(format!("cratarr-radio-recent-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("userdata.sqlite");
        let _ = std::fs::remove_file(&file);
        let u = crate::userdata::UserData::open(Some(&file)).unwrap();
        let now = crate::app::moment::now_secs();
        u.count_play("me", ("tr-1", "p1"), "al-1", &[], now - 60).unwrap();
        let along = crate::userdata::Heard { event: Some("f1".into()), followed: true, ..crate::userdata::Heard::counted(now - 30) };
        u.record("me", ("tr-2", "p2"), "al-1", &[], &along).unwrap();
        let recent = recent_plays(&file, "me").unwrap();
        assert_eq!(recent, [("tr-1".to_string(), now - 60)].into(), "a song heard while following may come again");
        drop(u);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn similar_by_is_the_account_s_setting_with_names_not_known_left_out() {
        let set = serde_json::json!({ "similar_by": ["genre", "nonsense", "artists"] });
        assert_eq!(similar_by_in(&set), Some(vec![Signal::Genres, Signal::Artists]));
        assert_eq!(similar_by_in(&serde_json::json!({ "similar_by": ["nonsense"] })), None);
        assert_eq!(similar_by_in(&serde_json::json!({})), None);
    }

    fn song(id: &str, album: u32, artist: u32, genre: u32, year: u32, playlists: &[u32]) -> Song {
        Song {
            id: id.into(),
            title: id.into(),
            album: format!("album {album}"),
            album_id: format!("al-{album}"),
            album_n: album,
            like: Track { artists: vec![artist], genres: vec![genre], year: Some(year), playlists: playlists.to_vec(), ..Track::default() },
            ..Song::default()
        }
    }

    /// Seed: album 1 (artist 1, genre 1, 1990, playlist 7). Kin: album 2 (same playlist, artist, genre, era).
    /// Strangers: albums 3.. (other artist, genre, 2020, no playlist).
    fn library() -> Index {
        let mut s = vec![song("s1", 1, 1, 1, 1990, &[7]), song("s2", 1, 1, 1, 1990, &[7]), song("k1", 2, 1, 1, 1991, &[7]), song("k2", 2, 2, 1, 1992, &[7])];
        for n in 0..40 {
            s.push(song(&format!("x{n}"), 3 + n, 9, 9, 2020, &[]));
        }
        Index::new(s)
    }

    #[test]
    fn a_list_s_closest_lists_its_kin_first_itself_left_out() {
        let index = library();
        let albums = |n: u32| (format!("al-{n}"), index.albums[&n].clone());
        let seed = index.albums[&1].clone();
        let candidates = vec![albums(3), albums(2), albums(4)];
        let found = closest_lists(&index, &seed, candidates, &Weights::EQUAL, 2);
        // SIM (change by design): al-3 shares no playlist, artist, genre or year with it, and nothing below the floor is shown.
        assert_eq!(found.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["al-2"], "the kin, nothing below the floor");
        assert!(found[0].1 > 70.0 && found[0].1 <= 100.0, "{found:?}");
        assert!(closest_lists(&index, &[], vec![albums(2)], &Weights::EQUAL, 2).is_empty(), "no seed in the library, none");
        assert!(closest_lists(&index, &seed, vec![("empty".into(), Vec::new())], &Weights::EQUAL, 2).is_empty(), "nothing of it in the library");
    }

    /// The library's file read as the radio reads it, a song with a whole-number BPM and an album artist among them (75:
    /// the two read in each other's places, and every library with a BPM failed to read).
    #[test]
    fn the_library_is_read_with_its_bpm_and_album_artist_in_their_places() {
        let dir = std::env::temp_dir().join(format!("cratarr-radio-read-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let lib = Connection::open(dir.join("library.sqlite")).unwrap();
        lib.execute_batch(
            "CREATE TABLE songs (id TEXT, title TEXT, artist TEXT, album TEXT, album_id TEXT, year INTEGER, genre TEXT,
               duration_ms INTEGER, bpm INTEGER, disc_no INTEGER, track_no INTEGER, rel TEXT, folder INTEGER);
             CREATE TABLE albums (id TEXT, genres TEXT, compilation INTEGER, artist TEXT, artist_id TEXT);
             CREATE TABLE song_artists (song_id TEXT, artist_id TEXT, position INTEGER);
             INSERT INTO songs VALUES ('tr-1', 'One', 'Ana', 'First', 'al-1', 1999, 'Jazz', 200000, 128, 1, 1, 'a/1.flac', 0);
             INSERT INTO songs VALUES ('tr-2', 'Two', 'Ana', 'First', 'al-1', 1999, 'Jazz', 180000, NULL, 1, 2, 'a/2.flac', 0);
             INSERT INTO albums VALUES ('al-1', '', 0, 'Ana', 'ar-ana');
             INSERT INTO song_artists VALUES ('tr-1', 'ar-ana', 0);",
        )
        .unwrap();
        Connection::open(dir.join("userdata.sqlite")).unwrap().execute_batch("CREATE TABLE album_tags (album TEXT, tag TEXT);").unwrap();
        drop(lib);
        let index = read_index(&dir.join("library.sqlite"), &dir.join("userdata.sqlite"), None, &HashMap::new()).unwrap();
        let bpms: Vec<Option<f32>> = index.songs.iter().map(|s| s.like.bpm).collect();
        assert_eq!(bpms, [Some(128.0), None]);
        assert_eq!(index.artist_ids, ["ar-ana"], "the album artist read as an artist");
        assert!(!index.songs[1].like.artists.is_empty(), "a song without its own artists has its album's");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_bandcamp_suggestion_takes_the_place_of_a_pick_it_is_more_alike_than() {
        let index = library();
        let picks: Vec<usize> = index.find(&["x0".to_string(), "x1".to_string(), "x2".to_string()]);
        let like = |artists: Vec<u32>, genres: Vec<u32>, year| Track { artists, genres, year: Some(year), ..Track::default() };
        let elsewhere = vec![
            Elsewhere { page: "https://a.bandcamp.com/album/far".into(), like: like(vec![], vec![], 2030) },
            Elsewhere { page: "https://a.bandcamp.com/album/near".into(), like: like(vec![1], vec![1], 1990) },
        ];
        let mixed = mix_elsewhere(&index, &ask(Mode::Similar), &picks, &elsewhere);
        assert_eq!(mixed.len(), 1, "{mixed:?}");
        assert_eq!(mixed[0].1, 1, "the one like the seed (its artist, genre and era)");
        assert!(mixed[0].0 < picks.len());
        assert!(mix_elsewhere(&index, &ask(Mode::Similar), &picks, &[]).is_empty());
    }

    #[test]
    fn more_music_is_the_libraries_chosen_and_whether_bandcamp_joins() {
        let s = serde_json::json!({ "more_music": { "libraries": ["/media/Music"], "bandcamp": true } });
        assert_eq!(more_music_in(&s), (Some(vec!["/media/Music".to_string()]), true));
        assert_eq!(more_music_in(&serde_json::json!({})), (None, false), "all libraries, no Bandcamp, by default");
    }

    fn ask(mode: Mode) -> Ask {
        Ask { mode, seed: vec!["s1".into(), "s2".into()], exclude: vec!["s1".into(), "s2".into()], last: Some("s2".into()), count: Some(2), albums: false, by: RadioBy::Track, skip_favourites: false, similar_by: None, folders: None }
    }

    #[test]
    fn within_takes_each_collection_the_page_names_as_a_play_reads_it() {
        use super::super::play::Collection;
        let within = |v: serde_json::Value| serde_json::from_value::<NextRequest>(serde_json::json!({ "within": v })).unwrap().within.map(|w| w.collection());
        let crate_ = Collection::Crate { id: "genre:Jazz".into(), favourites: None, favourite_albums: None, without: vec![], filter: vec![], order: super::super::CrateOrder::Own, down: false };
        assert_eq!(within(serde_json::json!({ "kind": "crate", "id": "genre:Jazz" })), Some(crate_), "a crate as saved");
        assert_eq!(within(serde_json::json!({ "kind": "playlist", "id": "Road" })), Some(Collection::Playlist { id: "Road".into() }));
        assert_eq!(within(serde_json::json!({ "kind": "album", "source": "cratarr", "id": "al-1" })), Some(Collection::Album { source: "cratarr".into(), id: "al-1".into() }));
        assert_eq!(within(serde_json::json!({ "kind": "artist", "source": "cratarr", "name": "Ana" })), Some(Collection::Artist { source: "cratarr".into(), name: "Ana".into(), id: None, without: Vec::new() }));
        let by_id = within(serde_json::json!({ "kind": "artist", "source": "cratarr", "name": "Rone", "id": "ar-2" }));
        assert_eq!(by_id, Some(Collection::Artist { source: "cratarr".into(), name: "Rone".into(), id: Some("ar-2".into()), without: Vec::new() }), "G6: by their id");
        let tracks = within(serde_json::json!({ "kind": "tracks", "source": "cratarr", "ids": ["tr-1", "tr-2"] }));
        assert_eq!(tracks, Some(Collection::Tracks { source: "cratarr".into(), ids: vec!["tr-1".into(), "tr-2".into()] }));
        assert_eq!(within(serde_json::Value::Null), None, "null: beyond it");
        assert!(serde_json::from_value::<NextRequest>(serde_json::json!({ "within": "c1" })).is_err(), "a crate's bare id no more");
    }

    /// Against copies of a real library and user data (RADIO_DATA: a folder with library.sqlite and userdata.sqlite):
    /// `RADIO_DATA=… cargo test real_library -- --ignored --nocapture`. Prints what each mode picks for a few albums.
    #[test]
    #[ignore]
    fn real_library() {
        let dir = std::path::PathBuf::from(std::env::var("RADIO_DATA").expect("RADIO_DATA"));
        let ud = read_only(&dir.join("userdata.sqlite")).unwrap();
        let owner: Option<String> = ud.query_row("SELECT user FROM annotation GROUP BY user ORDER BY count(*) DESC LIMIT 1", [], |r| r.get(0)).ok();
        let t = Instant::now();
        let index = read_index(&dir.join("library.sqlite"), &dir.join("userdata.sqlite"), owner.as_deref(), &HashMap::new()).unwrap();
        let recent = owner.as_deref().map(|o| recent_plays(&dir.join("userdata.sqlite"), o).unwrap()).unwrap_or_default();
        println!(
            "index: {} songs, {} albums, {} artists, {} favourites, {} with genres, {} with a year, {} with a length, {} recent plays, in {:?}",
            index.songs.len(),
            index.albums.len(),
            index.artist_ids.len(),
            index.songs.iter().filter(|s| s.favourite).count(),
            index.songs.iter().filter(|s| !s.like.genres.is_empty()).count(),
            index.songs.iter().filter(|s| s.like.year.is_some()).count(),
            index.songs.iter().filter(|s| s.duration_secs.is_some()).count(),
            recent.len(),
            t.elapsed()
        );
        let mut rng = Rng::seeded(42);
        let show = |i: usize| {
            let s = &index.songs[i];
            format!("{} - {} [{}] {}", s.artist.as_deref().unwrap_or("?"), s.title, s.album, s.like.year.map(|y| y.to_string()).unwrap_or_default())
        };
        let mut albums: Vec<&Vec<usize>> = index.albums.values().filter(|v| v.len() >= 6).collect();
        albums.sort_by_key(|v| index.songs[v[0]].album_id.clone());
        for album in albums.iter().step_by((albums.len() / 3).max(1)).take(3) {
            let seed: Vec<String> = album.iter().map(|&i| index.songs[i].id.clone()).collect();
            println!("\n== seed: {} ({} songs)", show(album[0]), album.len());
            let one = vec![seed[0].clone()];
            let req = Ask { mode: Mode::Radio, seed: one.clone(), exclude: seed.clone(), last: Some(one[0].clone()), count: Some(5), albums: false, by: RadioBy::Track, skip_favourites: false, similar_by: None, folders: None };
            println!("-- Radio's inserts after {}", show(album[0]));
            for i in choose(&index, &req, &recent, None, &mut rng) {
                println!("   {}", show(i));
            }
            for mode in [Mode::Similar, Mode::Era, Mode::Artist, Mode::Random] {
                let req = Ask { mode, seed: seed.clone(), exclude: seed.clone(), last: seed.last().cloned(), count: Some(5), albums: false, by: RadioBy::Track, skip_favourites: false, similar_by: None, folders: None };
                let t = Instant::now();
                let picked = choose(&index, &req, &recent, None, &mut rng);
                println!("-- {mode:?} ({:?})", t.elapsed());
                for i in picked {
                    println!("   {}", show(i));
                }
            }
        }
    }

    /// Radio's choices against a real library (RADIO_DATA as above): `cargo test real_radio_choices -- --ignored
    /// --nocapture`. For 300 tracks spread over the library: how often the inserts share the track's artist, how far
    /// they are in years, how often they come short, and that "Ignore favourites" lets none through; then a few examples.
    #[test]
    #[ignore]
    fn real_radio_choices() {
        let dir = std::path::PathBuf::from(std::env::var("RADIO_DATA").expect("RADIO_DATA"));
        let ud = read_only(&dir.join("userdata.sqlite")).unwrap();
        let owner: Option<String> = ud.query_row("SELECT user FROM annotation GROUP BY user ORDER BY count(*) DESC LIMIT 1", [], |r| r.get(0)).ok();
        let index = read_index(&dir.join("library.sqlite"), &dir.join("userdata.sqlite"), owner.as_deref(), &HashMap::new()).unwrap();
        let recent = owner.as_deref().map(|o| recent_plays(&dir.join("userdata.sqlite"), o).unwrap()).unwrap_or_default();
        let favourites = index.songs.iter().filter(|s| s.favourite).count();
        println!("index: {} songs, {} favourites, {} recent plays", index.songs.len(), favourites, recent.len());
        let step = (index.songs.len() / 300).max(1);
        let seeds: Vec<usize> = (0..index.songs.len()).step_by(step).take(300).collect();
        let ask = |seed: usize, by: RadioBy, skip: bool| Ask {
            mode: Mode::Radio,
            seed: vec![index.songs[seed].id.clone()],
            exclude: vec![index.songs[seed].id.clone()],
            last: Some(index.songs[seed].id.clone()),
            count: Some(5),
            albums: false,
            by,
            skip_favourites: skip,
            similar_by: None,
            folders: None,
        };
        let shares_artist = |a: usize, b: usize| index.songs[a].like.artists.iter().any(|x| index.songs[b].like.artists.contains(x));
        let mut rng = Rng::seeded(7);
        for by in [RadioBy::Track, RadioBy::Artist, RadioBy::Era] {
            for skip in [false, true] {
                let (mut picks, mut artist, mut short, mut favs, mut gaps, mut dated, mut albums_twice) = (0, 0, 0, 0, 0u32, 0, 0);
                let t = Instant::now();
                for &seed in &seeds {
                    let picked = choose(&index, &ask(seed, by, skip), &recent, None, &mut rng);
                    if picked.len() < 5 {
                        short += 1;
                    }
                    let mut seen_albums = HashSet::new();
                    for &i in &picked {
                        picks += 1;
                        artist += usize::from(shares_artist(seed, i));
                        favs += usize::from(index.songs[i].favourite);
                        if !seen_albums.insert(index.songs[i].album_id.clone()) {
                            albums_twice += 1;
                        }
                        if let (Some(a), Some(b)) = (index.songs[seed].like.year, index.songs[i].like.year) {
                            gaps += a.abs_diff(b);
                            dated += 1;
                        }
                    }
                }
                println!(
                    "{by:?}{}: {} picks for {} tracks in {:?}; same artist {:.0}%; mean year gap {:.1}; short {}; favourites {}; an album twice {}",
                    if skip { " + ignore favourites" } else { "" },
                    picks,
                    seeds.len(),
                    t.elapsed(),
                    100.0 * artist as f64 / picks.max(1) as f64,
                    f64::from(gaps) / f64::from(dated.max(1)),
                    short,
                    favs,
                    albums_twice
                );
            }
        }
        let show = |i: usize| {
            let s = &index.songs[i];
            format!("{} - {} [{}] {}{}", s.artist.as_deref().unwrap_or("?"), s.title, s.album, s.like.year.map(|y| y.to_string()).unwrap_or_default(), if s.favourite { " ♥" } else { "" })
        };
        for &seed in seeds.iter().step_by(75) {
            println!("
== after {}", show(seed));
            for by in [RadioBy::Track, RadioBy::Artist, RadioBy::Era] {
                println!("-- {by:?}");
                for i in choose(&index, &ask(seed, by, false), &recent, None, &mut rng) {
                    println!("   {}", show(i));
                }
            }
        }
    }

    /// One object from the player: what the picker takes and what the server keeps read side by side, and a player can
    /// name neither the folders nor the account.
    #[test]
    fn a_request_is_the_pickers_part_and_the_servers_in_one_object() {
        let sent = r#"{"mode":"radio","by":"artist","seed":["a"],"exclude":["a","b"],"last":"a","count":10,"albums":true,
            "skip_favourites":true,"similar_by":["genre"],"within":{"kind":"playlist","id":"Road"},"folders":[1],"account":"other"}"#;
        let q: NextRequest = serde_json::from_str(sent).unwrap();
        assert_eq!((q.ask.mode, q.ask.by, q.ask.count, q.ask.albums, q.ask.skip_favourites), (Mode::Radio, RadioBy::Artist, Some(10), true, true));
        assert_eq!((q.ask.seed, q.ask.exclude, q.ask.last), (vec!["a".to_string()], vec!["a".to_string(), "b".to_string()], Some("a".to_string())));
        assert_eq!(q.ask.similar_by, Some(vec![Signal::Genres]));
        assert_eq!(q.within, Some(Within::Playlist { id: "Road".into() }));
        assert_eq!((q.ask.folders, q.account), (None, None), "the server's to say");
        let bare: NextRequest = serde_json::from_str("{}").unwrap();
        assert_eq!((bare.ask.mode, bare.ask.by, bare.ask.count, bare.within), (Mode::Similar, RadioBy::Track, None, None));
        assert!(serde_json::from_str::<NextRequest>(r#"{"mode":"nonsense"}"#).is_err(), "a mode not known is refused, as it was");
    }

    #[test]
    fn autoplay_and_the_request_read_as_players_send_them() {
        let a: Autoplay = serde_json::from_str(r#"{"mode":"artist","albums":true,"seed":["tr-1"]}"#).unwrap();
        assert_eq!(a, Autoplay { mode: AutoMode::Artist, albums: true, restart: false, seed: vec!["tr-1".into()], within: None });
        assert_eq!(serde_json::to_string(&Autoplay::default()).unwrap(), r#"{"mode":"similar","albums":false,"restart":false}"#);
        let q: NextRequest = serde_json::from_str(r#"{"mode":"radio","seed":["a"],"exclude":["a"],"last":"a","count":5}"#).unwrap();
        assert_eq!((q.ask.mode, q.ask.albums), (Mode::Radio, false));
        assert_eq!(from_old(Some(Mode::AlbumMix)), (AutoMode::Similar, true, false));
        assert_eq!(from_old(Some(Mode::Continuous)), (AutoMode::Similar, false, false));
    }

    // --- SIM (SCENARIOS-SIM.md): the shown lists count what the seed knows, hold the floor and something telling ---

    use crate::similarity::{ALIKE_FLOOR, Library};

    /// Names as numbers, as the library's reading numbers them.
    #[derive(Default)]
    struct Numbered(HashMap<String, u32>);

    impl Numbered {
        fn of(&mut self, name: &str) -> u32 {
            let next = self.0.len() as u32;
            *self.0.entry(name.to_string()).or_insert(next)
        }

        fn list(&self) -> Vec<String> {
            let mut l = vec![String::new(); self.0.len()];
            for (name, &n) in &self.0 {
                l[n as usize] = name.clone();
            }
            l
        }
    }

    /// A song as the scenarios write one: its album, artists (the album artist among them), words and numbers by name.
    #[derive(Clone, Default)]
    struct Listed {
        id: String,
        album: String,
        artists: Vec<String>,
        genre: Option<&'static str>,
        style: Option<&'static str>,
        playlists: Vec<&'static str>,
        year: Option<u32>,
        bpm: Option<f32>,
        /// Energy, danceability and every mood: AudioMuse heard it.
        heard: Option<f32>,
        sound: Option<Vec<f32>>,
    }

    /// Near a sound: a little off it, by the track's number.
    fn near(centre: [f32; 4], j: usize) -> Vec<f32> {
        let off = [0.0, 0.03 * (j % 3) as f32, 0.03 * ((j + 1) % 3) as f32, 0.02 * (j % 2) as f32];
        centre.iter().zip(off).map(|(c, o)| c + o).collect()
    }

    const C_P: [f32; 4] = [1.0, 0.0, 0.0, 0.0];
    const C_Q: [f32; 4] = [0.92, 0.39, 0.0, 0.0];
    const C_T: [f32; 4] = [0.0, 0.0, 1.0, 0.0];
    const C_H: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

    /// The index of these songs, as the server's reading and the facts make it: its artists, genres and playlists numbered, the
    /// sounds in its library.
    fn index_of(listed: &[Listed], playlists: &[&str]) -> Index {
        let (mut artists, mut genres, mut styles, mut lists, mut albums) = (Numbered::default(), Numbered::default(), Numbered::default(), Numbered::default(), Numbered::default());
        for p in playlists {
            lists.of(p);
        }
        let songs: Vec<Song> = listed
            .iter()
            .map(|l| Song {
                id: l.id.clone(),
                title: l.id.clone(),
                album: l.album.clone(),
                album_id: l.album.clone(),
                album_n: albums.of(&l.album),
                like: Track {
                    playlists: l.playlists.iter().map(|p| lists.of(p)).collect(),
                    artists: l.artists.iter().map(|a| artists.of(a)).collect(),
                    genres: l.genre.map(|g| genres.of(g)).into_iter().collect(),
                    styles: l.style.map(|s| styles.of(s)).into_iter().collect(),
                    year: l.year,
                    bpm: l.bpm,
                    energy: l.heard,
                    danceability: l.heard,
                    moods: l.heard.map(|x| [x; 5]),
                    ..Track::default()
                },
                folders: vec![0],
                ..Song::default()
            })
            .collect();
        let sounds = listed.iter().map(|l| l.sound.clone().map(Arc::from)).collect();
        let mut index = Index::new(songs);
        index.like = Arc::new(Library::new(index.songs.iter().map(|s| &s.like), sounds));
        index.artist_ids = artists.list();
        index.genre_ids = genres.list();
        index.playlist_ids = lists.list();
        index
    }

    /// F-LISTS (SCENARIOS-SIM): 20 filler albums; P and its kin Q, the sparse R, the stranger T, co, Y, h and z; `kin` more albums
    /// like Q (al-q2 …); and, `plus`, SIM.20's albums. Artists are `ar-<name>`, albums `al-<name>`, playlists P, Q, R, T, Y.
    fn f_lists(kin: usize, plus: bool) -> Index {
        let mut l: Vec<Listed> = Vec::new();
        let song = |id: String, album: &str, artists: &[&str]| Listed { id, album: album.into(), artists: artists.iter().map(|a| format!("ar-{a}")).collect(), ..Listed::default() };
        for a in 0..20 {
            for t in 0..10 {
                let block = a % 4;
                let name = format!("f{a}");
                l.push(Listed {
                    genre: Some("Rock"),
                    year: Some([1980, 1990, 2010, 2020][block]),
                    bpm: Some([40.0, 80.0, 120.0, 160.0][block]),
                    ..song(format!("f{a}-{t}"), &format!("al-{name}"), &[name.as_str()])
                });
            }
        }
        // P's values, Q's and T's, for track j (1 to 8).
        let p = |j: usize, base: Listed| Listed {
            genre: Some("Ambient"),
            style: Some("drone"),
            year: Some(1994 + j as u32),
            bpm: Some(90.0 + (j - 1) as f32 * 20.0 / 7.0),
            heard: Some(0.3 + 0.01 * (j % 3) as f32),
            sound: Some(near(C_P, j)),
            ..base
        };
        let q = |j: usize, base: Listed| Listed { year: Some(1995 + j as u32), sound: Some(near(C_Q, j)), ..p(j, base) };
        let t = |j: usize, base: Listed| Listed {
            genre: Some("Metal"),
            year: Some(2015 + ((j - 1) % 5) as u32),
            bpm: Some(150.0 + (j - 1) as f32 * 10.0 / 7.0),
            heard: Some(0.95),
            sound: Some(near(C_T, j)),
            ..base
        };
        for j in 1..=8 {
            let p1 = j == 1;
            l.push(p(j, Listed { playlists: if p1 { vec!["P", "Q"] } else { vec!["P"] }, ..song(format!("p{j}"), "al-p", if p1 { &["a", "co"][..] } else { &["a"][..] }) }));
            l.push(q(j, Listed { playlists: vec!["Q"], ..song(format!("q{j}"), "al-q", &["b"]) }));
            l.push(Listed { year: Some(1994 + j as u32), playlists: vec!["R"], ..song(format!("r{j}"), "al-r", &["c"]) });
            l.push(t(j, Listed { playlists: vec!["T"], ..song(format!("t{j}"), "al-t", &["d"]) }));
            l.push(Listed { year: Some(1998), ..song(format!("y{j}"), "al-y", &["y"]) });
            l.push(Listed { year: Some(1998), playlists: vec!["Y"], ..song(format!("y{}", j + 8), "al-y2", &["y"]) });
            l.push(Listed { heard: Some(0.98), sound: Some(near(C_H, j)), ..song(format!("h{j}"), "al-h", &["h"]) });
            for n in 2..2 + kin {
                l.push(q(j, song(format!("q{n}-{j}"), &format!("al-q{n}"), &[format!("k{n}").as_str()])));
            }
            if plus {
                l.push(q(j, song(format!("qg{j}"), "al-qg", if p1 { &["b2", "a"][..] } else { &["b2"][..] })));
                l.push(q(j, song(format!("qn{j}"), "al-qn", &["b3"])));
                l.push(t(j, song(format!("tg{j}"), "al-tg", if p1 { &["d2", "a"][..] } else { &["d2"][..] })));
                let other = format!("e{j}");
                l.push(p(j, song(format!("comp{j}"), "al-comp", &[if j <= 2 { "a" } else { other.as_str() }])));
                l.push(p(j, song(format!("comp0-{j}"), "al-comp0", &[format!("g{j}").as_str()])));
                l.push(p(j, song(format!("a2-{j}"), "al-a2", &["a"])));
            }
        }
        for j in 1..=4 {
            l.push(Listed { year: Some(1994 + j as u32), ..song(format!("co{j}"), "al-co", &["co"]) });
        }
        for j in 1..=3 {
            l.push(Listed { year: Some(2001), ..song(format!("z{j}"), "al-z", &["z"]) });
        }
        index_of(&l, &["P", "Q", "R", "T", "Y"])
    }

    fn songs_of(index: &Index, album: &str) -> Vec<usize> {
        (0..index.songs.len()).filter(|&i| index.songs[i].album_id == album).collect()
    }

    fn named(found: &[(String, f64)]) -> Vec<&str> {
        found.iter().map(|(id, _)| id.as_str()).collect()
    }

    /// The lists as `similar_lists` hands them in: a list's songs, by the songs' ids.
    fn lists(index: &Index, of: &[(&str, &[&str])]) -> Vec<(String, Vec<usize>)> {
        of.iter().map(|(id, prefixes)| (id.to_string(), (0..index.songs.len()).filter(|&i| prefixes.iter().any(|p| index.songs[i].like.playlists.contains(&(index.playlist_number(p).unwrap())))).collect())).collect()
    }

    /// SIM.3 at Similar albums: a sparse album no longer beats a rich right one.
    #[test]
    fn a_sparse_candidate_never_beats_a_rich_right_one() {
        let mut l: Vec<Listed> = (0..200)
            .map(|i| {
                let block = i % 4;
                Listed {
                    id: format!("f{i}"),
                    album: format!("al-f{i}"),
                    artists: vec![format!("ar-f{i}")],
                    genre: Some("Rock"),
                    year: Some([1980, 1990, 2010, 2020][block]),
                    bpm: Some([40.0, 80.0, 120.0, 160.0][block]),
                    heard: None,
                    ..Listed::default()
                }
            })
            .collect();
        for n in 0..10 {
            let one = |name: &str| Listed { id: format!("{name}{n}"), album: format!("al-{name}"), genre: Some("g3"), ..Listed::default() };
            l.push(Listed { year: Some(2000), bpm: Some(100.0), sound: Some(C_P.to_vec()), ..one("s3") });
            l.push(Listed { year: Some(2010), bpm: Some(120.0), sound: Some(vec![0.81, (1.0f32 - 0.81 * 0.81).sqrt(), 0.0, 0.0]), ..one("r3") });
            l.push(Listed { year: Some(2000), ..one("p3") });
        }
        let mut index = index_of(&l, &[]);
        // Energy alone of AudioMuse's numbers, as F-KINDS' s3, R3 and P3 have it.
        for s in &mut index.songs {
            let energy = match s.album_id.as_str() {
                "al-s3" => Some(0.5),
                "al-r3" => Some(0.75),
                "al-p3" => None,
                _ => s.like.year.map(|y| [0.0, 0.25, 0.75, 1.0][[1980, 1990, 2010, 2020].iter().position(|x| *x == y).unwrap()]),
            };
            s.like.energy = energy;
        }
        let sounds = (0..index.songs.len()).map(|i| index.like.sound(i).map(Arc::from)).collect();
        index.like = Arc::new(Library::new(index.songs.iter().map(|s| &s.like), sounds));
        for (s, reach) in [(Signal::Era, 20.0), (Signal::Tempo, 40.0), (Signal::Energy, 0.5)] {
            assert_eq!(index.like.reach(s), Some(reach), "{s:?}");
        }
        let found = albums_like(&index, "al-s3", &Weights::EQUAL, 12);
        assert_eq!(found, [("al-r3".to_string(), 60.0), ("al-p3".to_string(), 40.0)], "the rich right one first; both share the genre");
    }

    /// SIM.6: a seed known by too little shows nothing, not even one of its own year.
    #[test]
    fn a_seed_known_by_too_little_shows_nothing() {
        let index = f_lists(0, false);
        for album in ["al-y", "al-h"] {
            assert!(albums_like(&index, album, &Weights::EQUAL, 12).is_empty(), "{album}");
        }
        let crates = |names: &[&str]| -> Vec<(String, Vec<usize>)> { names.iter().map(|a| (format!("C-{}", &a[3..]), songs_of(&index, a))).collect() };
        assert!(closest_lists(&index, &songs_of(&index, "al-y"), crates(&["al-p", "al-q", "al-r", "al-t"]), &Weights::EQUAL, 12).is_empty(), "C-Y");
        let y = songs_of(&index, "al-y2");
        let others = lists(&index, &[("P", &["P"]), ("Q", &["Q"]), ("R", &["R"]), ("T", &["T"])]);
        assert!(closest_playlists(&index, &y, "Y", others, &Weights::EQUAL, 12).is_empty(), "playlist Y: its own membership is no kind");
        assert_eq!(artists_like(&index, "ar-z", None, &HashMap::new(), &mut Rng::seeded(1), 20), Vec::<String>::new(), "z, by its year alone");
    }

    /// SIM.7 at the lists: nothing below the floor to make up the count, the count still capping.
    #[test]
    fn nothing_below_the_floor_is_shown() {
        let index = f_lists(15, false);
        let kin: HashSet<String> = std::iter::once("al-q".to_string()).chain((2..17).map(|n| format!("al-q{n}"))).collect();
        let twelve = albums_like(&index, "al-p", &Weights::EQUAL, 12);
        assert!(twelve.len() == 12 && twelve.iter().all(|(id, _)| kin.contains(id)), "{twelve:?}");
        let all = albums_like(&index, "al-p", &Weights::EQUAL, 30);
        assert_eq!(all.iter().map(|(id, _)| id.clone()).collect::<HashSet<String>>(), kin, "its 16 kin, nothing below the floor: {all:?}");
        assert!(all.iter().all(|(_, alike)| *alike >= ALIKE_FLOOR * 100.0));
    }

    /// SIM.11: Similar albums, playlists and crates count what the seed knows: Q's, never R's.
    #[test]
    fn each_lists_similar_lists_count_what_the_seed_knows() {
        let index = f_lists(0, false);
        assert_eq!(named(&albums_like(&index, "al-p", &Weights::EQUAL, 12)), ["al-q"], "albums");
        let others = lists(&index, &[("Q", &["Q"]), ("R", &["R"]), ("T", &["T"]), ("Y", &["Y"])]);
        let p = songs_of(&index, "al-p");
        assert_eq!(named(&closest_playlists(&index, &p, "P", others, &Weights::EQUAL, 12)), ["Q"], "playlists");
        let crates: Vec<(String, Vec<usize>)> = ["al-q", "al-r", "al-t", "al-y"].iter().map(|a| (format!("C-{}", &a[3..]), songs_of(&index, a))).collect();
        assert_eq!(named(&closest_lists(&index, &p, crates, &Weights::EQUAL, 12)), ["C-q"], "crates");
        // R reads below the floor on each, and has nothing telling; al-co shares a credit (telling) but reads about 0.1.
        let seed = index.profile(&p);
        let albums = Weights::EQUAL.with(Signal::Tracks, 0.0);
        for album in ["al-r", "al-co"] {
            let alike = seed.to_profile(&index.like, &index.profile(&songs_of(&index, album))).score(&albums);
            assert!((alike - 0.1).abs() < 0.01 && alike < ALIKE_FLOOR, "{album}: {alike}");
        }
    }

    /// SIM.12: Similar artists are only those with a song alike enough on something telling.
    #[test]
    fn similar_artists_are_only_those_alike_enough() {
        let index = f_lists(0, false);
        for n in 0..10 {
            assert_eq!(artists_like(&index, "ar-a", None, &HashMap::new(), &mut Rng::seeded(n), 20), ["ar-b"], "draw {n}");
        }
        // co's songs share p1's credit and rank above R's, their twins without it, but stay below the floor.
        let p = index.profile(&index.of_item("ar-a"));
        let (co, r) = (index.score(&p, index.by_id["co3"], &Weights::EQUAL), index.score(&p, index.by_id["r3"], &Weights::EQUAL));
        assert!(co > r && (co - 0.114).abs() < 0.01 && co < ALIKE_FLOOR, "co {co}, r {r}");
        assert_eq!(artists_like(&index, "ar-z", None, &HashMap::new(), &mut Rng::seeded(1), 20), Vec::<String>::new(), "z: SIM.6");
    }

    /// SIM.20: an album sharing an artist (a guest, a compilation's) ranks higher; those sharing none keep their order; the own
    /// artist's albums stay out.
    #[test]
    fn a_shared_artist_ranks_an_album_higher_and_the_rest_keep_their_order() {
        let index = f_lists(0, true);
        let found = named(&albums_like(&index, "al-p", &Weights::EQUAL, 12)).into_iter().map(String::from).collect::<Vec<_>>();
        let at = |id: &str| found.iter().position(|x| x == id).unwrap_or_else(|| panic!("{id} in {found:?}"));
        assert!(at("al-qg") < at("al-qn"), "a guest on one track: {found:?}");
        assert!(at("al-comp") < at("al-comp0"), "a compilation, two tracks by the artist: {found:?}");
        assert!(!found.contains(&"al-tg".to_string()), "a shared credit, well under the floor: {found:?}");
        assert!(!found.contains(&"al-a2".to_string()), "the own artist's: More by shows it");
        // Those sharing no artist, in the order their scores without Artists give.
        let seed = index.profile(&songs_of(&index, "al-p"));
        let without = Weights::EQUAL.with(Signal::Tracks, 0.0).with(Signal::Artists, 0.0);
        let mut alone: Vec<(&str, f64)> = ["al-qn", "al-comp0", "al-q"].iter().map(|a| (*a, seed.to_profile(&index.like, &index.profile(&songs_of(&index, a))).score(&without))).collect();
        alone.sort_by(|a, b| b.1.total_cmp(&a.1));
        let shown: Vec<&str> = found.iter().map(String::as_str).filter(|id| ["al-qn", "al-comp0", "al-q"].contains(id)).collect();
        assert_eq!(shown, alone.iter().map(|a| a.0).collect::<Vec<_>>(), "{alone:?}");
    }

    /// F-SHELF as the server's index reads it: the catalogue's facts of each track (`with_facts`), its records under served ids.
    fn f_shelf(split: bool) -> Index {
        type Record = (&'static str, usize, &'static str, &'static str, u32, Option<f64>, &'static [&'static str], bool);
        let mut records: Vec<Record> = vec![
            ("al-rg", 14, "Hip-Hop/Rap", "Hip Hop", 2026, None, &[], false),
            ("al-aft", 15, "", "", 2025, Some(120.0), &["dance", "electronic", "france", "french"], true),
            ("al-crash", 4, "", "", 2025, None, &[], false),
            ("al-open", 9, "", "", 2024, Some(100.0), &[], true),
            ("al-n26a", 10, "", "", 2026, None, &[], false),
            ("al-n26b", 10, "", "", 2026, None, &[], false),
            ("al-n26c", 10, "", "", 2026, None, &[], false),
            ("al-rap26", 10, "rap", "Hip Hop", 2026, None, &[], false),
            ("al-hh24", 10, "hip hop", "Hip Hop", 2024, None, &[], false),
            ("al-hh19", 10, "hip-hop", "Hip Hop", 2019, Some(90.0), &[], true),
            ("al-el26", 10, "electronic", "Electronic", 2026, Some(120.0), &[], true),
        ];
        for (n, year) in [1990; 9].into_iter().chain([2000; 9]).chain([2013; 18]).enumerate() {
            records.push((Box::leak(format!("al-rock{n}").into_boxed_str()), 10, "rock", "Rock", year, None, &[], false));
        }
        let (mut songs, mut facts, mut track) = (Vec::new(), Vec::new(), 0i64);
        for (r, (id, tracks, before, now, year, bpm, tags, heard)) in records.iter().enumerate() {
            let genre = if split { *now } else { *before };
            for _ in 0..*tracks {
                track += 1;
                songs.push(Song { id: crate::serve::ids::song(track), album_id: id.to_string(), album_n: r as u32, like: Track { artists: vec![r as u32], ..Track::default() }, folders: vec![0], ..Song::default() });
                facts.push(crate::crates::Facts {
                    rel: format!("{id}/{track}.mp3"),
                    track: Some(track),
                    genres: (!genre.is_empty()).then(|| genre.to_string()).into_iter().collect(),
                    tags: tags.iter().map(|t| t.to_string()).collect(),
                    year: Some(f64::from(*year)),
                    bpm: *bpm,
                    energy: heard.then_some(50.0),
                    sound: heard.then(|| Arc::from(near(C_P, track as usize))),
                    ..crate::crates::Facts::default()
                });
            }
        }
        let index = Index::new(songs).with_facts(&facts);
        assert_eq!(index.like.reach(Signal::Era), Some(13.0), "F-SHELF's year reach, as production's");
        index
    }

    /// SIM.9 through the index: before GEN, Rafting Goods shows nothing.
    #[test]
    fn rafting_goods_before_the_genre_split() {
        let found = albums_like(&f_shelf(false), "al-rg", &Weights::EQUAL, 12);
        assert!(found.is_empty(), "nothing, not noise: {found:?}");
    }

    /// SIM.10 (and SIM.7's Similar albums scale) through the index: after GEN, only its kin, Tracks out of the mean and Artists in.
    #[test]
    fn rafting_goods_after_the_genre_split() {
        let found = albums_like(&f_shelf(true), "al-rg", &Weights::EQUAL, 12);
        assert_eq!(named(&found), ["al-rap26", "al-hh24", "al-hh19"], "{found:?}");
        assert_eq!(found[0].1, 66.7, "(0 + 1 + 1) / 3, not (0 + 0 + 1 + 1) / 4");
        assert!(albums_like(&f_shelf(false), "al-rg", &Weights::EQUAL, 12).is_empty(), "neither fix alone");
    }

    /// SIM.16: similar songs are a radio: as many as asked while the library holds them, a seed known by its year alone too.
    #[test]
    fn similar_songs_are_a_radio() {
        let mut songs = library().songs;
        songs.push(Song { id: "e".into(), album_id: "al-99".into(), album_n: 99, like: Track { year: Some(2000), ..Track::default() }, ..Song::default() });
        let index = Index::new(songs);
        for n in 0..20 {
            assert_eq!(songs_like(&index, "e", 5, None, &HashMap::new(), &mut Rng::seeded(n)).len(), 5, "e, by its year alone");
            let req = Ask { mode: Mode::Similar, seed: vec!["s1".into()], exclude: vec!["s1".into()], last: None, count: Some(5), ..Ask::default() };
            let radio: Vec<String> = choose(&index, &req, &HashMap::new(), None, &mut Rng::seeded(n)).into_iter().map(|i| index.songs[i].id.clone()).collect();
            assert_eq!(songs_like(&index, "s1", 5, None, &HashMap::new(), &mut Rng::seeded(n)), radio, "ordered as the radio orders them");
        }
    }

    /// SIM.16 and SIM.22: a Bandcamp suggestion is compared with the picks on what it can be known by (artists, genres, year),
    /// its genre numbered as the songs' after the facts.
    #[test]
    fn bandcamps_suggestions_are_compared_like_for_like() {
        // As the library's reading numbers genres, Rock before Jazz; the facts meet Jazz first.
        let mut songs = Vec::new();
        let mut facts = Vec::new();
        for t in 1..=30i64 {
            let jazz = t <= 6;
            songs.push(Song { id: crate::serve::ids::song(t), album_id: format!("al-{t}"), album_n: t as u32, like: Track { artists: vec![(t - 1) as u32], genres: vec![u32::from(jazz)], ..Track::default() }, ..Song::default() });
            facts.push(crate::crates::Facts {
                rel: format!("{t}.flac"),
                track: Some(t),
                genres: vec![if jazz { "Jazz" } else { "Rock" }.to_string()],
                year: Some(if jazz { 1974.0 + t as f64 } else { 1960.0 + (t * 2) as f64 }),
                bpm: Some(if jazz { 118.0 + t as f64 } else { 80.0 + (t * 3) as f64 }),
                energy: Some(if jazz { 40.0 + t as f64 } else { (t * 3) as f64 }),
                sound: Some(Arc::from(near(if jazz { C_P } else { C_T }, t as usize))),
                ..crate::crates::Facts::default()
            });
        }
        let mut index = Index::new(songs);
        index.genre_ids = vec!["rock".into(), "jazz".into()];
        let mut index = index.with_facts(&facts);
        index.artist_ids = (1..=30).map(crate::serve::ids::artist).collect();
        let suggestion: super::super::discover::Suggestion = serde_json::from_value(serde_json::json!({
            "key": "e", "album": "E", "artist": "M", "artist_key": "m", "year": 1975, "release_group": null, "cover": null, "links": [],
            "new_artist": false, "why": [], "bandcamp": "https://m.bandcamp.com/album/e", "genres": ["Jazz"]
        }))
        .unwrap();
        let artist_ids: HashMap<String, i64> = [(crate::serve::ids::artist_key("M"), 1)].into();
        let elsewhere = elsewhere_of(&index, &[suggestion], &artist_ids, &[]);
        let jazz = index.songs[0].like.genres.clone();
        assert_eq!(elsewhere[0].like.genres, jazz, "its Jazz is the songs' Jazz");
        let req = Ask { mode: Mode::Similar, seed: vec![index.songs[0].id.clone()], exclude: vec![index.songs[0].id.clone()], last: None, count: Some(5), ..Ask::default() };
        let picks: Vec<usize> = (1..6).collect();
        let mixed = mix_elsewhere(&index, &req, &picks, &elsewhere);
        assert_eq!(mixed.iter().map(|m| m.1).collect::<Vec<_>>(), [0], "E takes the least alike pick's place: {mixed:?}");
    }
}
