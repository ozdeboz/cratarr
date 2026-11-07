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
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use super::ToggleError;
#[cfg(test)]
use super::play::Rng;
use super::playon::QueueTrack;
use crate::similarity::{Signal, Track, Weights, unique};

pub use cratarr_rules::radio::{Ask, Index, MAX_COUNT, Mode, RadioBy, Song, choose};

#[cfg(test)]
mod proof;

/// The lane of Radio's inserts (`QueueTrack::lane`): right after the own track they are like (and its hand-added run).
pub const LANE_RADIO: &str = "radio";
/// The lane of Autoplay's tracks: after everything, when the queue runs out.
pub const LANE_AUTOPLAY: &str = "autoplay";

/// Autoplay's modes; off is its absence (`stop` in commands).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum AutoMode {
    #[default]
    Similar,
    Artist,
    Era,
    Random,
    Taste,
}

impl AutoMode {
    /// What Autoplay in this mode asks for.
    pub fn asks(self) -> Mode {
        match self {
            AutoMode::Similar => Mode::Similar,
            AutoMode::Artist => Mode::Artist,
            AutoMode::Era => Mode::Era,
            AutoMode::Random => Mode::Random,
            AutoMode::Taste => Mode::Taste,
        }
    }
}

/// Autoplay's mode in a command: `stop` switches it off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum AutoplayMode {
    Stop,
    Similar,
    Artist,
    Era,
    Random,
    Taste,
}

impl AutoplayMode {
    pub fn on(self) -> Option<AutoMode> {
        match self {
            AutoplayMode::Stop => None,
            AutoplayMode::Similar => Some(AutoMode::Similar),
            AutoplayMode::Artist => Some(AutoMode::Artist),
            AutoplayMode::Era => Some(AutoMode::Era),
            AutoplayMode::Random => Some(AutoMode::Random),
            AutoplayMode::Taste => Some(AutoMode::Taste),
        }
    }
    pub fn of(mode: Option<AutoMode>) -> Self {
        match mode {
            None => AutoplayMode::Stop,
            Some(AutoMode::Similar) => AutoplayMode::Similar,
            Some(AutoMode::Artist) => AutoplayMode::Artist,
            Some(AutoMode::Era) => AutoplayMode::Era,
            Some(AutoMode::Random) => AutoplayMode::Random,
            Some(AutoMode::Taste) => AutoplayMode::Taste,
        }
    }
}

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

/// What Radio and Autoplay stay inside (decision 0094: the page's collection), as Cratarr's own library has it: a
/// crate as saved (a genre too, `genre:<name>`), a playlist, a record, an artist, or tracks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Within {
    /// `without`: the libraries switched off, as the crate showed (Home's crates are of every library).
    Crate {
        id: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[cfg_attr(test, ts(as = "Option<_>", optional))]
        without: Vec<String>,
    },
    Playlist { id: String },
    Album { source: String, id: String },
    /// `without`: the libraries switched off, as the artist showed.
    Artist {
        source: String,
        name: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[cfg_attr(test, ts(as = "Option<_>", optional))]
        without: Vec<String>,
    },
    Tracks { source: String, ids: Vec<String> },
}

impl Within {
    /// As a play names it (app::play), to read its tracks as a play does.
    fn collection(&self) -> super::play::Collection {
        use super::play::Collection;
        match self.clone() {
            Within::Crate { id, without } => {
                Collection::Crate { id, favourites: None, favourite_albums: None, without, filter: Vec::new(), order: super::CrateOrder::Own, down: false }
            }
            Within::Playlist { id } => Collection::Playlist { id },
            Within::Album { source, id } => Collection::Album { source, id },
            Within::Artist { source, name, without } => Collection::Artist { source, name, without },
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

/// An account's own "Similar by" (its settings' `similar_by`: signal names, the first counting most), when it set one;
/// names not known are left out. A player Cratarr drives (Play On, with nobody asking) takes its account's.
pub fn similar_by_of(user: &str) -> Option<Vec<Signal>> {
    let served = crate::serve::instance()?;
    let (_, settings) = served.userdata.settings_of(user).ok()?;
    similar_by_in(&settings)
}

/// An account's More music choice (0.9.5, its settings' `more_music`): the libraries Radio and Autoplay choose from
/// (by Cratarr's library ids, its folders; none: all) and whether Discover's Bandcamp suggestions join them.
pub fn more_music_of(user: &str) -> (Option<Vec<String>>, bool) {
    match crate::serve::instance().and_then(|s| s.userdata.settings_of(user).ok()) {
        Some((_, settings)) => more_music_in(&settings),
        None => (None, false),
    }
}

fn more_music_in(settings: &serde_json::Value) -> (Option<Vec<String>>, bool) {
    let m = &settings["more_music"];
    (m["libraries"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()), m["bandcamp"].as_bool().unwrap_or(false))
}

/// "Similar by" as an account's settings hold it; none set, or none known: none.
fn similar_by_in(settings: &serde_json::Value) -> Option<Vec<Signal>> {
    let names = settings.get("similar_by")?.as_array()?.clone();
    Some(names.into_iter().filter_map(|n| serde_json::from_value(n).ok()).collect()).filter(|v: &Vec<Signal>| !v.is_empty())
}

/// Played within this long: left out while there is enough else (the owner, 2026-10-16).
const RECENT: Duration = Duration::from_secs(7 * 24 * 3600);
/// An index older than this is built again even when the library is not.
const FRESH: Duration = Duration::from_secs(600);

// --- the index, from the library's and the user data's own files ---

struct Cache {
    index: Arc<Index>,
    built_from: String,
    at: Instant,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// The album artist Radio takes a song's artists to go on with, for a query over `albums a`. A compilation's ("Various Artists")
/// is no artist: it would make every compilation alike.
pub(crate) const ALBUM_ARTIST_HEARD: &str = "CASE WHEN a.compilation = 1 OR lower(a.artist) IN ('various artists', 'various', 'va', 'verschiedene interpreten') THEN NULL ELSE a.artist_id END";

/// A request as its listener's settings make it: their Similar by, and the libraries they let Radio and Autoplay choose
/// from (0.9.5: their folders). With the account it is for, and whether Bandcamp joins.
fn as_asked(mut req: NextRequest) -> Result<(NextRequest, Option<String>, bool), ToggleError> {
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
pub async fn next(app: &Arc<super::App>, req: NextRequest) -> Result<Vec<QueueTrack>, ToggleError> {
    let (index, recent) = ready().await?;
    let (req, account, bandcamp) = as_asked(req)?;
    let within = match &req.within {
        Some(w) => Some(index.find(&super::play::own_songs(app, &w.collection(), account.as_deref()).await?)),
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
    let (found, artist_ids) = match tokio::task::spawn_blocking(move || anyhow::Ok((super::discover::kept(&s, &a)?, s.artist_ids()?))).await {
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
        let streams: Vec<&crate::bandcamp::Track> = album.tracks.iter().filter(|t| t.stream.is_some()).collect();
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
/// like each is to what the account skips (none while its skipped source is off: `PlaylistService::skipped_fits`).
async fn with_taste(app: &Arc<super::App>, index: Arc<Index>, account: Option<&str>, taste: bool) -> Result<Arc<Index>, ToggleError> {
    let Some(user) = account else { return Ok(index) };
    let fits = if taste { app.playlists.taste_fits(user, &[]).await? } else { HashMap::new() };
    let skipped = app.playlists.skipped_fits(user, &[]).await?;
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
        tokio::task::spawn_blocking(move || built_from(&library)).await.map_err(|e| anyhow::anyhow!("{e}"))??
    };
    if let Some(c) = CACHE.lock().unwrap_or_else(|p| p.into_inner()).as_ref()
        && c.built_from == built_from
        && c.at.elapsed() < FRESH
    {
        return Ok(c.index.clone());
    }
    // The playlists' songs: every account's playlists' lines through the catalogue (what people put together).
    let lines = match served.playlists() {
        Some(p) => p.snapshot_all().await.map_err(|e| anyhow::anyhow!("reading the playlists: {e:?}"))?,
        None => Vec::new(),
    };
    let (served2, dir) = (served.clone(), dir.to_path_buf());
    let index = tokio::task::spawn_blocking(move || -> Result<Index> {
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
        read_index(&dir.join("library.sqlite"), &dir.join("userdata.sqlite"), None, &member).map(|i| i.with_facts(&facts))
    })
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))??;
    let index = Arc::new(index);
    *CACHE.lock().unwrap_or_else(|p| p.into_inner()) = Some(Cache { index: index.clone(), built_from, at: Instant::now() });
    Ok(index)
}

fn read_only(file: &Path) -> Result<Connection> {
    let conn = crate::db::open_read_only(file)?;
    conn.busy_timeout(Duration::from_secs(10))?;
    Ok(conn)
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

/// Songs played within seven days as one's own, with when (epoch seconds): what was heard while following another
/// is not kept from the radio (the owner, 2026-10-30).
fn recent_plays(userdata: &Path, owner: &str) -> Result<HashMap<String, i64>> {
    let conn = read_only(userdata)?;
    let since = super::moment::now_secs() - RECENT.as_secs() as i64;
    let mut st = conn.prepare("SELECT item, own_date FROM annotation WHERE user = ?1 AND kind = 'song' AND own_date >= ?2")?;
    let rows = st.query_map(params![owner, since], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// What Radio leaves for later of `user`'s, as `ready` reads it: nothing while its play history is off.
pub(super) fn recent_of(serve: &crate::serve::Serve, user: &str) -> Result<HashMap<String, i64>> {
    let Some(dir) = &serve.data_dir else { return Ok(HashMap::new()) };
    if serve.userdata.tracking(user) { recent_plays(&dir.join("userdata.sqlite"), user) } else { Ok(HashMap::new()) }
}

/// The folders of the libraries `user` lets Radio and Autoplay choose from; none: every one.
pub(super) fn folders_of(serve: &crate::serve::Serve, user: &str) -> Option<Vec<i64>> {
    let libraries = more_music_of(user).0?;
    Some(serve.library.roots().into_iter().filter(|(_, r)| libraries.iter().any(|l| Path::new(l) == Path::new(r))).map(|(f, _)| f).collect())
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
    let seed: Vec<String> = index.of_item(item).into_iter().map(|i| index.songs[i].id.clone()).collect();
    if seed.is_empty() {
        return Ok(Vec::new());
    }
    let req = Ask { mode: Mode::Similar, exclude: seed.clone(), seed, last: None, count: Some(count.min(MAX_COUNT)), albums: false, by: RadioBy::Track, skip_favourites: false, similar_by: similar_by_of_listener(), folders: None };
    Ok(choose(&index, &req, &recent, None, &mut super::play::rng()).into_iter().map(|i| index.songs[i].id.clone()).collect())
}

/// Lists like a list, of its own kind (the owner, 2026-10-23: "any list that has a profile at the top gets a
/// similarity set at the bottom"): each candidate's profile against the seed's (`similarity::Profile::to_profile`), by
/// the listener's "Similar by" (else all signals alike), the closest first, as 0 to 100. Candidates with none of their
/// songs in the library are left out.
fn closest_lists(index: &Index, seed: &[usize], candidates: Vec<(String, Vec<usize>)>, weights: &Weights, count: usize) -> Vec<(String, f64)> {
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
    tokio::task::spawn_blocking(move || {
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
        closest_lists(&index, &seed, candidates, &weights, count)
    })
    .await
    .map_err(|e| ToggleError::from(anyhow::anyhow!("{e}")))
}

/// Lists like a list of songs (their OpenSubsonic ids), among `candidates` (an id and its songs' ids), the closest first,
/// with how alike (0 to 100): a playlist's similar playlists.
pub async fn similar_lists(seed: Vec<String>, candidates: Vec<(String, Vec<String>)>, count: usize) -> Result<Vec<(String, f64)>, ToggleError> {
    let (index, _) = ready().await?;
    let weights = listener_weights();
    tokio::task::spawn_blocking(move || {
        let seed = index.find(&seed);
        let candidates = candidates.into_iter().map(|(id, songs)| (id, index.find(&songs))).collect();
        closest_lists(&index, &seed, candidates, &weights, count)
    })
    .await
    .map_err(|e| ToggleError::from(anyhow::anyhow!("{e}")))
}

/// Artists like an artist: the artists of the songs Similar finds for it, the most frequent first.
pub async fn similar_artists(artist: &str, count: usize) -> Result<Vec<String>, ToggleError> {
    let (index, recent) = ready().await?;
    let Some(own) = index.artist_ids.iter().position(|x| x == artist).map(|a| a as u32) else { return Ok(Vec::new()) };
    let seed: Vec<String> = index.of_item(artist).into_iter().map(|i| index.songs[i].id.clone()).collect();
    let req = Ask { mode: Mode::Similar, exclude: seed.clone(), seed, last: None, count: Some(MAX_COUNT), albums: false, by: RadioBy::Track, skip_favourites: false, similar_by: similar_by_of_listener(), folders: None };
    let mut tally: HashMap<u32, u32> = HashMap::new();
    for i in choose(&index, &req, &recent, None, &mut super::play::rng()) {
        for &a in unique(&index.songs[i].like.artists).collect::<Vec<_>>().iter().filter(|&&a| a != own) {
            *tally.entry(a).or_default() += 1;
        }
    }
    let mut ranked: Vec<(u32, u32)> = tally.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Ok(ranked.into_iter().take(count).map(|(a, _)| index.artist_ids[a as usize].clone()).collect())
}

/// The index with the listener's favourites and exclusions marked, and their recent plays, for a choice made here: the
/// running request's account's, whose player asks (decision 0051).
async fn ready() -> Result<(Arc<Index>, HashMap<String, i64>), ToggleError> {
    let served = crate::serve::instance().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
    let dir = served.data_dir.clone().ok_or_else(|| ToggleError::Refused("Radio needs Cratarr's data folder.".into()))?;
    let index = index(&served, &dir).await?;
    let user = served.current_user();
    let excluded = match served.playlists() {
        Some(p) => p.excluded_songs_of(user.as_deref()).await?,
        None => HashSet::new(),
    };
    let (recent, starred) = match user {
        Some(user) => tokio::task::spawn_blocking(move || -> Result<_> {
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
        assert_eq!(found.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["al-2", "al-3"], "the kin first, then the count");
        assert!(found[0].1 > found[1].1 && found[0].1 <= 100.0);
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
        assert_eq!(within(serde_json::json!({ "kind": "artist", "source": "cratarr", "name": "Ana" })), Some(Collection::Artist { source: "cratarr".into(), name: "Ana".into(), without: Vec::new() }));
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
}
