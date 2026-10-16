//! Radio (decision 0036): what comes after the queue, from the owner's own library. The player that plays asks for the
//! next few tracks (`POST /api/radio/next`) when fewer than three are left ahead of the playing one; Cratarr chooses.
//!
//! Cratarr's own library only. The index below reads the library and the user data from their own SQLite files, read
//! only, and the playlists from the playlist service; it is built again when the library is, and at least every ten
//! minutes (playlists, album tags and favourites change in between).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};

use super::ToggleError;
use super::playon::QueueTrack;

/// The lane of radio tracks (`QueueTrack::lane`): after the album or playlist and what was added by hand.
pub const LANE_RADIO: &str = "radio";

/// How the radio chooses (decision 0036). Similar is the owner's default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Close to the seed on playlists, artist, genre, era and tags, weighed equally; drifts as the closest are played.
    #[default]
    Similar,
    /// Close to the playing track only: it wanders.
    Continuous,
    /// A whole album at a time, the closest to the seed.
    AlbumMix,
    /// Within five years of the playing track.
    Era,
    /// The playing track's artists and the seed's most common ones.
    Artist,
    /// The seed again, in a fresh order.
    Reshuffle,
    /// At random from the seed, then from the whole library.
    Random,
}

/// The radio as the queue keeps it (`Queue::radio`): on while there is one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Radio {
    #[serde(default)]
    pub mode: Mode,
    /// The tracks it stays close to (ids), when started from an item ("Start radio"); empty: the queue's own album or
    /// playlist tracks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seed: Vec<String>,
}

/// What a player asks for.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NextRequest {
    #[serde(default)]
    pub mode: Mode,
    /// The radio's seed, else the queue's own album or playlist tracks.
    #[serde(default)]
    pub seed: Vec<String>,
    /// Every track already in the queue.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// The playing track.
    #[serde(default)]
    pub last: Option<String>,
    /// How many (about five; Album mix gives one album).
    #[serde(default)]
    pub count: Option<usize>,
}

const DEFAULT_COUNT: usize = 5;
const MAX_COUNT: usize = 50;
/// Played within this long: left out while there is enough else (the owner, 2026-10-16).
const RECENT: Duration = Duration::from_secs(7 * 24 * 3600);
/// Similar picks among this many of the best.
const TOP: usize = 30;
/// An index older than this is built again even when the library is not.
const FRESH: Duration = Duration::from_secs(600);

/// One song as the radio sees it; names interned as numbers.
#[derive(Debug, Clone, Default)]
pub(crate) struct Song {
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: String,
    pub album_id: String,
    /// The album, interned.
    pub album_n: u32,
    /// Its artists and its album artist, interned.
    pub artists: Vec<u32>,
    pub genres: Vec<u32>,
    pub year: Option<u32>,
    pub playlists: Vec<u32>,
    /// Its album's tags.
    pub tags: Vec<u32>,
    pub favourite: bool,
}

/// The library, ready to choose from.
#[derive(Debug, Default)]
pub(crate) struct Index {
    pub songs: Vec<Song>,
    by_id: HashMap<String, usize>,
    /// Each album's songs in their order.
    albums: HashMap<u32, Vec<usize>>,
}

impl Index {
    pub(crate) fn new(songs: Vec<Song>) -> Self {
        let by_id = songs.iter().enumerate().map(|(i, s)| (s.id.clone(), i)).collect();
        let mut albums: HashMap<u32, Vec<usize>> = HashMap::new();
        for (i, s) in songs.iter().enumerate() {
            albums.entry(s.album_n).or_default().push(i);
        }
        Self { songs, by_id, albums }
    }

    fn find(&self, ids: &[String]) -> Vec<usize> {
        ids.iter().filter_map(|id| self.by_id.get(id).copied()).collect()
    }
}

/// A small generator (xorshift, seeded from the clock): picking needs no better.
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new() -> Self {
        Self(SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) | 1)
    }
    #[cfg(test)]
    pub(crate) fn seeded(n: u64) -> Self {
        Self(n | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    /// From 0 up to (not including) 1.
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            items.swap(i, self.below(i + 1));
        }
    }
}

/// What the seed is like: how many of its songs have each playlist, artist, genre and tag, and its middle year.
struct Profile {
    n: f64,
    playlists: HashMap<u32, u32>,
    artists: HashMap<u32, u32>,
    genres: HashMap<u32, u32>,
    tags: HashMap<u32, u32>,
    year: Option<u32>,
}

impl Profile {
    fn of(index: &Index, seed: &[usize]) -> Self {
        let mut p = Profile { n: seed.len() as f64, playlists: HashMap::new(), artists: HashMap::new(), genres: HashMap::new(), tags: HashMap::new(), year: None };
        let mut years = Vec::new();
        for &i in seed {
            let s = &index.songs[i];
            for (map, items) in [(&mut p.playlists, &s.playlists), (&mut p.artists, &s.artists), (&mut p.genres, &s.genres), (&mut p.tags, &s.tags)] {
                for x in unique(items) {
                    *map.entry(x).or_default() += 1;
                }
            }
            years.extend(s.year);
        }
        years.sort_unstable();
        p.year = years.get(years.len() / 2).copied();
        p
    }

    /// 0 to 1 on each of the five signals, weighed equally (the owner, 2026-10-16).
    fn score(&self, s: &Song) -> f64 {
        if self.n == 0.0 {
            return 0.0;
        }
        let most = |map: &HashMap<u32, u32>, items: &[u32]| items.iter().filter_map(|x| map.get(x)).copied().max().unwrap_or(0) as f64;
        // A playlist holding the whole seed is the strongest sign; an artist, genre or tag shared by a quarter of it counts in full.
        let playlists = (most(&self.playlists, &s.playlists) / self.n).min(1.0);
        let quarter = |c: f64| (4.0 * c / self.n).min(1.0);
        let artist = quarter(most(&self.artists, &s.artists));
        let genre = quarter(most(&self.genres, &s.genres));
        let tags = quarter(most(&self.tags, &s.tags));
        let era = match (self.year, s.year) {
            (Some(a), Some(b)) => (1.0 - (a as f64 - b as f64).abs() / 10.0).max(0.0),
            _ => 0.0,
        };
        (playlists + artist + genre + era + tags) / 5.0
    }
}

fn unique(items: &[u32]) -> impl Iterator<Item = u32> + '_ {
    items.iter().enumerate().filter(move |(i, x)| !items[..*i].contains(x)).map(|(_, x)| *x)
}

/// The radio's choice for one request: songs of the index, in the order they should play. `recent`: songs played
/// within seven days, with when (epoch seconds).
pub(crate) fn choose(index: &Index, req: &NextRequest, recent: &HashMap<String, i64>, rng: &mut Rng) -> Vec<usize> {
    let count = req.count.unwrap_or(DEFAULT_COUNT).clamp(1, MAX_COUNT);
    let seed = index.find(&req.seed);
    let last = req.last.as_ref().and_then(|id| index.by_id.get(id).copied());
    if req.mode == Mode::Reshuffle {
        return reshuffle(&seed, last, rng);
    }
    let mut out: HashSet<usize> = index.find(&req.exclude).into_iter().collect();
    out.extend(seed.iter().copied());
    out.extend(last);
    // Not in the queue, not the seed's own (decision 0036, rule 8); the recently played only when there is not enough else.
    let fresh: Vec<usize> = (0..index.songs.len()).filter(|i| !out.contains(i) && !recent.contains_key(&index.songs[*i].id)).collect();
    // Not enough else: as many of the recently played as are missing come back, those played longest ago first.
    let pool = if fresh.len() >= count {
        fresh
    } else {
        let mut stale: Vec<usize> = (0..index.songs.len()).filter(|i| !out.contains(i) && recent.contains_key(&index.songs[*i].id)).collect();
        stale.sort_by_key(|i| recent.get(&index.songs[*i].id).copied().unwrap_or(0));
        let need = count - fresh.len();
        fresh.into_iter().chain(stale.into_iter().take(need)).collect()
    };
    let artists_of = |i: usize| index.songs[i].artists.clone();
    match req.mode {
        Mode::Similar | Mode::Continuous => {
            let base = if req.mode == Mode::Continuous { last.into_iter().collect() } else { seed.clone() };
            let base = if base.is_empty() { last.into_iter().collect() } else { base };
            let profile = Profile::of(index, &base);
            let mut scored: Vec<(usize, f64)> = pool
                .iter()
                .map(|&i| {
                    let s = &index.songs[i];
                    (i, profile.score(s) + if s.favourite { 0.05 } else { 0.0 } + rng.unit() * 0.1)
                })
                .collect();
            scored.sort_by(|a, b| b.1.total_cmp(&a.1));
            scored.truncate(TOP.max(count * 3));
            pick_spaced(index, scored, count, last.map(artists_of), true, rng)
        }
        Mode::AlbumMix => album_mix(index, &pool, &seed, &out, rng),
        Mode::Era => {
            let center = last.and_then(|i| index.songs[i].year).or_else(|| Profile::of(index, &seed).year);
            let near: Vec<(usize, f64)> = pool
                .iter()
                .filter(|&&i| match (center, index.songs[i].year) {
                    (Some(c), Some(y)) => c.abs_diff(y) <= 5,
                    _ => false,
                })
                .map(|&i| (i, rng.unit()))
                .collect();
            pick_spaced(index, near, count, last.map(artists_of), true, rng)
        }
        Mode::Artist => {
            let mut wanted: HashSet<u32> = last.map(artists_of).unwrap_or_default().into_iter().collect();
            let profile = Profile::of(index, &seed);
            let mut common: Vec<(u32, u32)> = profile.artists.into_iter().collect();
            common.sort_by(|a, b| b.1.cmp(&a.1));
            wanted.extend(common.into_iter().take(3).map(|(a, _)| a));
            let theirs: Vec<(usize, f64)> =
                pool.iter().filter(|&&i| index.songs[i].artists.iter().any(|a| wanted.contains(a))).map(|&i| (i, rng.unit())).collect();
            // Their artists on purpose, so only the albums are spaced.
            pick_spaced(index, theirs, count, None, false, rng)
        }
        Mode::Random => {
            let seeds: HashSet<usize> = seed.iter().copied().collect();
            let queued: HashSet<usize> = index.find(&req.exclude).into_iter().collect();
            // From the seed first (its own tracks again, those not in the queue), then from the whole library.
            let from_seed: Vec<(usize, f64)> = seed.iter().copied().filter(|i| !queued.contains(i) && Some(*i) != last).map(|i| (i, rng.unit())).collect();
            let mut picked = pick_spaced(index, from_seed, count, None, false, rng);
            if picked.len() < count {
                let rest: Vec<(usize, f64)> = pool.iter().filter(|i| !seeds.contains(i) && !picked.contains(i)).map(|&i| (i, rng.unit())).collect();
                picked.extend(pick_spaced(index, rest, count - picked.len(), None, true, rng));
            }
            picked
        }
        Mode::Reshuffle => unreachable!("answered above"),
    }
}

/// The seed again in a fresh order, never starting with the track that just played.
fn reshuffle(seed: &[usize], last: Option<usize>, rng: &mut Rng) -> Vec<usize> {
    let mut order: Vec<usize> = seed.to_vec();
    rng.shuffle(&mut order);
    if order.len() > 1 && order.first() == last.as_ref() {
        let n = order.len();
        order.swap(0, n - 1);
    }
    order
}

/// Up to `count` of `candidates` (by weight, the higher the likelier), no two of one album, and (with `vary_artist`)
/// never the same artist twice in a row; the rules give way when nothing else is left.
fn pick_spaced(index: &Index, mut candidates: Vec<(usize, f64)>, count: usize, before: Option<Vec<u32>>, vary_artist: bool, rng: &mut Rng) -> Vec<usize> {
    let mut picked: Vec<usize> = Vec::new();
    let mut albums: HashSet<u32> = HashSet::new();
    let mut prev: Vec<u32> = before.unwrap_or_default();
    while picked.len() < count && !candidates.is_empty() {
        let fits = |i: usize| {
            let s = &index.songs[i];
            !albums.contains(&s.album_n) && (!vary_artist || !s.artists.iter().any(|a| prev.contains(a)))
        };
        let pool: Vec<usize> = (0..candidates.len()).filter(|&k| fits(candidates[k].0)).collect();
        let pool = if pool.is_empty() { (0..candidates.len()).collect() } else { pool };
        // Weighted by score: the best are likeliest, the others still possible.
        let total: f64 = pool.iter().map(|&k| candidates[k].1.max(0.001)).sum();
        let mut at = rng.unit() * total;
        let mut chosen = pool[pool.len() - 1];
        for &k in &pool {
            at -= candidates[k].1.max(0.001);
            if at <= 0.0 {
                chosen = k;
                break;
            }
        }
        let (i, _) = candidates.swap_remove(chosen);
        albums.insert(index.songs[i].album_n);
        prev = index.songs[i].artists.clone();
        picked.push(i);
    }
    picked
}

/// One whole album, the closest to the seed of those with nothing in the queue and none of the seed's own.
fn album_mix(index: &Index, pool: &[usize], seed: &[usize], out: &HashSet<usize>, rng: &mut Rng) -> Vec<usize> {
    let profile = Profile::of(index, seed);
    let banned: HashSet<u32> = out.iter().map(|&i| index.songs[i].album_n).collect();
    let mut sums: HashMap<u32, (f64, u32)> = HashMap::new();
    for &i in pool {
        let s = &index.songs[i];
        if banned.contains(&s.album_n) {
            continue;
        }
        let e = sums.entry(s.album_n).or_default();
        e.0 += profile.score(s);
        e.1 += 1;
    }
    // Albums of at least two tracks (Finamp's Album mix); singles only when there is nothing else.
    let mut albums: Vec<(u32, f64)> = sums.iter().filter(|(_, (_, n))| *n >= 2).map(|(a, (sum, n))| (*a, sum / *n as f64 + rng.unit() * 0.05)).collect();
    if albums.is_empty() {
        albums = sums.iter().map(|(a, (sum, n))| (*a, sum / *n as f64 + rng.unit() * 0.05)).collect();
    }
    albums.sort_by(|a, b| b.1.total_cmp(&a.1));
    albums.truncate(10);
    let Some(&(album, _)) = albums.get(if albums.is_empty() { 0 } else { rng.below(albums.len().min(3)) }) else { return Vec::new() };
    index.albums.get(&album).map(|songs| songs.iter().copied().filter(|i| !out.contains(i)).collect()).unwrap_or_default()
}

// --- the index, from the library's and the user data's own files ---

struct Cache {
    index: Arc<Index>,
    built_from: String,
    at: Instant,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// The next radio tracks for a player (decision 0036).
pub async fn next(req: NextRequest) -> Result<Vec<QueueTrack>, ToggleError> {
    let served = crate::serve::instance().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
    let dir = served.data_dir.clone().ok_or_else(|| ToggleError::Refused("Radio needs Cratarr's data folder.".into()))?;
    let owner = served.users.owner();
    let index = index(&served, &dir).await?;
    let picked = tokio::task::spawn_blocking(move || -> Result<Vec<QueueTrack>> {
        let recent = match &owner {
            Some(o) => recent_plays(&dir.join("userdata.sqlite"), o)?,
            None => HashMap::new(),
        };
        let chosen = choose(&index, &req, &recent, &mut Rng::new());
        Ok(chosen.into_iter().map(|i| track(&index.songs[i])).collect())
    })
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))??;
    Ok(picked)
}

fn track(s: &Song) -> QueueTrack {
    QueueTrack {
        lane: Some(LANE_RADIO.into()),
        source: crate::source::cratarr::KIND.id.to_string(),
        id: s.id.clone(),
        name: s.title.clone(),
        artist: s.artist.clone(),
        album: Some(s.album.clone()).filter(|a| !a.is_empty()),
        album_id: Some(s.album_id.clone()),
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
    // The playlists' songs: each playlist's lines through the catalogue, as the Subsonic playlists are read.
    let lines = match served.playlists() {
        Some(p) => p.snapshot().await.map_err(|e| anyhow::anyhow!("reading the playlists: {e:?}"))?,
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
        let owner = served2.users.owner();
        read_index(&dir.join("library.sqlite"), &dir.join("userdata.sqlite"), owner.as_deref(), &member)
    })
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))??;
    let index = Arc::new(index);
    *CACHE.lock().unwrap_or_else(|p| p.into_inner()) = Some(Cache { index: index.clone(), built_from, at: Instant::now() });
    Ok(index)
}

fn read_only(file: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(file, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .with_context(|| format!("opening {} to read", file.display()))?;
    conn.busy_timeout(Duration::from_secs(10))?;
    Ok(conn)
}

/// What the library was last built from (its meta row), so an index from before is built again.
fn built_from(library: &Path) -> Result<String> {
    let conn = read_only(library)?;
    Ok(conn.query_row("SELECT value FROM meta WHERE key = 'built_from'", [], |r| r.get(0)).unwrap_or_default())
}

/// Songs played within seven days, with when (epoch seconds).
fn recent_plays(userdata: &Path, owner: &str) -> Result<HashMap<String, i64>> {
    let conn = read_only(userdata)?;
    let since = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0) - RECENT.as_secs() as i64;
    let mut st = conn.prepare("SELECT item, play_date FROM annotation WHERE user = ?1 AND kind = 'song' AND play_date >= ?2")?;
    let rows = st.query_map(params![owner, since], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Interns names as numbers.
#[derive(Default)]
struct Names(HashMap<String, u32>);

impl Names {
    fn of(&mut self, name: &str) -> u32 {
        let n = self.0.len() as u32;
        *self.0.entry(name.to_lowercase()).or_insert(n)
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
    let starred: HashSet<String> = match owner {
        Some(o) => {
            let mut st = ud.prepare("SELECT item FROM annotation WHERE user = ?1 AND kind = 'song' AND starred = 1")?;
            st.query_map(params![o], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<_>>()?
        }
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
    let (mut albums, mut artists, mut genres, mut tags) = (Names::default(), Names::default(), Names::default(), Names::default());
    let mut st = lib.prepare(
        "SELECT s.id, s.title, s.artist, s.album, s.album_id, s.year, s.genre, a.genres, a.artist_id
         FROM songs s LEFT JOIN albums a ON a.id = s.album_id
         ORDER BY s.album_id, s.disc_no, s.track_no, s.rel",
    )?;
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
            r.get::<_, Option<String>>(8)?,
        ))
    })?;
    for r in rows {
        let (id, title, artist, album, album_id, year, genre, album_genres, album_artist) = r?;
        let mut a: Vec<u32> = artists_of.get(&id).map(|v| v.iter().map(|x| artists.of(x)).collect()).unwrap_or_default();
        a.extend(album_artist.as_deref().map(|x| artists.of(x)));
        let mut g: Vec<u32> = genre.as_deref().filter(|x| !x.is_empty()).map(|x| genres.of(x)).into_iter().collect();
        g.extend(album_genres.as_deref().unwrap_or("").split('\u{1f}').filter(|x| !x.is_empty()).map(|x| genres.of(x)));
        let t: Vec<u32> = tags_of.get(&album_id).map(|v| v.iter().map(|x| tags.of(x)).collect()).unwrap_or_default();
        songs.push(Song {
            favourite: starred.contains(&id),
            playlists: member.get(&id).cloned().unwrap_or_default(),
            album_n: albums.of(&album_id),
            artists: a,
            genres: g,
            tags: t,
            year: year.and_then(|y| u32::try_from(y).ok()).filter(|y| *y > 0),
            id,
            title,
            artist,
            album,
            album_id,
        });
    }
    Ok(Index::new(songs))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: &str, album: u32, artist: u32, genre: u32, year: u32, playlists: &[u32]) -> Song {
        Song {
            id: id.into(),
            title: id.into(),
            album: format!("album {album}"),
            album_id: format!("al-{album}"),
            album_n: album,
            artists: vec![artist],
            genres: vec![genre],
            year: Some(year),
            playlists: playlists.to_vec(),
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

    fn ids(index: &Index, picked: &[usize]) -> Vec<String> {
        picked.iter().map(|&i| index.songs[i].id.clone()).collect()
    }

    fn ask(mode: Mode) -> NextRequest {
        NextRequest { mode, seed: vec!["s1".into(), "s2".into()], exclude: vec!["s1".into(), "s2".into()], last: Some("s2".into()), count: Some(2) }
    }

    #[test]
    fn similar_puts_the_kin_first_and_leaves_out_the_seed() {
        let index = library();
        let picked = ids(&index, &choose(&index, &ask(Mode::Similar), &HashMap::new(), &mut Rng::seeded(7)));
        assert_eq!(picked.len(), 2);
        assert!(!picked.contains(&"s1".to_string()) && !picked.contains(&"s2".to_string()), "{picked:?}");
        // The kin score near 0.9, the strangers near 0: a stranger's chance among 30 is small but not nil, so seeds vary.
        assert!(picked.contains(&"k1".to_string()) || picked.contains(&"k2".to_string()), "{picked:?}");
    }

    #[test]
    fn recently_played_wait_while_there_is_enough_else() {
        let index = library();
        let recent: HashMap<String, i64> = [("k1".to_string(), 100), ("k2".to_string(), 50)].into();
        for n in 0..20 {
            let picked = ids(&index, &choose(&index, &ask(Mode::Similar), &recent, &mut Rng::seeded(n)));
            assert!(!picked.iter().any(|p| p.starts_with('k')), "{picked:?}");
        }
        // Nothing else left: the recently played come back, the longest ago first.
        let tiny = Index::new(vec![song("s1", 1, 1, 1, 1990, &[]), song("k1", 2, 1, 1, 1990, &[]), song("k2", 3, 1, 1, 1990, &[])]);
        let req = NextRequest { count: Some(1), ..ask(Mode::Random) };
        assert_eq!(ids(&tiny, &choose(&tiny, &NextRequest { seed: vec!["s1".into()], exclude: vec!["s1".into()], ..req }, &recent, &mut Rng::seeded(3))), ["k2"]);
    }

    #[test]
    fn spacing_keeps_albums_and_artists_apart() {
        let index = library();
        for n in 0..20 {
            let picked = choose(&index, &NextRequest { count: Some(5), ..ask(Mode::Random) }, &HashMap::new(), &mut Rng::seeded(n));
            let albums: HashSet<u32> = picked.iter().map(|&i| index.songs[i].album_n).collect();
            assert_eq!(albums.len(), picked.len(), "two of one album: {:?}", ids(&index, &picked));
        }
    }

    #[test]
    fn album_mix_gives_one_whole_album_not_the_seeds() {
        let index = library();
        let picked = ids(&index, &choose(&index, &ask(Mode::AlbumMix), &HashMap::new(), &mut Rng::seeded(1)));
        assert_eq!(picked, ["k1", "k2"], "the closest album, whole and in order");
    }

    #[test]
    fn reshuffle_plays_the_seed_again_never_starting_with_the_last() {
        let index = Index::new((0..6).map(|n| song(&format!("s{n}"), 1, 1, 1, 1990, &[])).collect());
        let seed: Vec<String> = (0..6).map(|n| format!("s{n}")).collect();
        for n in 0..30 {
            let req = NextRequest { mode: Mode::Reshuffle, seed: seed.clone(), exclude: seed.clone(), last: Some("s3".into()), count: None };
            let picked = ids(&index, &choose(&index, &req, &HashMap::new(), &mut Rng::seeded(n)));
            assert_eq!(picked.len(), 6);
            assert_ne!(picked[0], "s3");
        }
    }

    #[test]
    fn era_stays_within_five_years() {
        let mut s = vec![song("now", 1, 1, 1, 1990, &[])];
        s.extend((0..10).map(|n| song(&format!("near{n}"), 10 + n, 2 + n, 1, 1986 + n, &[])));
        s.extend((0..10).map(|n| song(&format!("far{n}"), 30 + n, 2 + n, 1, 2010 + n, &[])));
        let index = Index::new(s);
        let req = NextRequest { mode: Mode::Era, seed: vec![], exclude: vec!["now".into()], last: Some("now".into()), count: Some(5) };
        let picked = choose(&index, &req, &HashMap::new(), &mut Rng::seeded(5));
        assert_eq!(picked.len(), 5);
        assert!(picked.iter().all(|&i| index.songs[i].year.unwrap().abs_diff(1990) <= 5));
    }

    #[test]
    fn artist_keeps_to_the_artists() {
        let index = library();
        let req = NextRequest { mode: Mode::Artist, count: Some(5), ..ask(Mode::Artist) };
        let picked = ids(&index, &choose(&index, &req, &HashMap::new(), &mut Rng::seeded(2)));
        assert_eq!(picked, ["k1"], "only k1 shares artist 1; k2 is artist 2: {picked:?}");
    }

    #[test]
    fn the_mode_and_radio_read_as_players_send_them() {
        let r: Radio = serde_json::from_str(r#"{"mode":"album_mix","seed":["tr-1"]}"#).unwrap();
        assert_eq!(r, Radio { mode: Mode::AlbumMix, seed: vec!["tr-1".into()] });
        assert_eq!(serde_json::to_string(&Radio::default()).unwrap(), r#"{"mode":"similar"}"#);
        let q: NextRequest = serde_json::from_str(r#"{"mode":"era","exclude":["a"],"last":"a"}"#).unwrap();
        assert_eq!((q.mode, q.count), (Mode::Era, None));
    }
}
