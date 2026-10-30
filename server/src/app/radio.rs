//! Radio and Autoplay (decision 0036, amended): the music goes on from the owner's own library. The player that plays
//! asks Cratarr (`POST /api/radio/next`) and fills two lanes: Radio's inserts, about five tracks like each of the
//! queue's own tracks right after it (`mode: "radio"`), and Autoplay's tail, appended when fewer than three tracks are
//! left (`mode: similar | artist | era | random`, `albums` for whole albums). Cratarr chooses; the player keeps both
//! switches and fills its lanes.
//!
//! Cratarr's own library only. The index below reads the library and the user data from their own SQLite files, read
//! only, and the playlists from the playlist service; it is built again when the library is, and at least every ten
//! minutes (playlists and album tags change in between). The listener's favourites and plays are read for each choice:
//! the account whose player asks (decision 0051).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::ToggleError;
use super::playon::QueueTrack;
use crate::similarity::{Library, Profile, Signal, Track, Weights, unique};

/// The lane of Radio's inserts (`QueueTrack::lane`): right after the own track they are like (and its hand-added run).
pub const LANE_RADIO: &str = "radio";
/// The lane of Autoplay's tracks: after everything, when the queue runs out.
pub const LANE_AUTOPLAY: &str = "autoplay";

/// How Cratarr chooses, as a player asks: Radio's inserts, or one of Autoplay's modes. Continuous, AlbumMix and
/// Reshuffle are the modes from before the amendment, still taken from apps not yet updated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "RadioMode")]
pub enum Mode {
    /// Close to the seed on playlists, artist, genre, era and tags, weighed equally; drifts as the closest are played.
    #[default]
    Similar,
    /// The playing track's artists and the seed's most common ones.
    Artist,
    /// Within five years of the playing track.
    Era,
    /// At random from the seed, then from the whole library.
    Random,
    /// Radio's inserts: like the one seed track, its artist weighed double.
    Radio,
    /// Before the amendment: close to the playing track only.
    Continuous,
    /// Before the amendment: a whole album at a time (now `albums`).
    AlbumMix,
    /// Before the amendment: the seed again, in a fresh order (now Autoplay's `restart`, which the player does).
    Reshuffle,
}

impl Mode {
    /// What Autoplay in `mode` asks for.
    pub fn of(mode: AutoMode) -> Self {
        match mode {
            AutoMode::Similar => Mode::Similar,
            AutoMode::Artist => Mode::Artist,
            AutoMode::Era => Mode::Era,
            AutoMode::Random => Mode::Random,
        }
    }
}

/// Autoplay's modes; off is its absence (`stop` in commands).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AutoMode {
    #[default]
    Similar,
    Artist,
    Era,
    Random,
}

/// Autoplay's mode in a command: `stop` switches it off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AutoplayMode {
    Stop,
    Similar,
    Artist,
    Era,
    Random,
}

impl AutoplayMode {
    pub fn on(self) -> Option<AutoMode> {
        match self {
            AutoplayMode::Stop => None,
            AutoplayMode::Similar => Some(AutoMode::Similar),
            AutoplayMode::Artist => Some(AutoMode::Artist),
            AutoplayMode::Era => Some(AutoMode::Era),
            AutoplayMode::Random => Some(AutoMode::Random),
        }
    }
    pub fn of(mode: Option<AutoMode>) -> Self {
        match mode {
            None => AutoplayMode::Stop,
            Some(AutoMode::Similar) => AutoplayMode::Similar,
            Some(AutoMode::Artist) => AutoplayMode::Artist,
            Some(AutoMode::Era) => AutoplayMode::Era,
            Some(AutoMode::Random) => AutoplayMode::Random,
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

/// What Radio's inserts are like (decision 0036, amended): the track (its artist weighed double), its artist (as
/// Autoplay's Artist, seeded by the track) or its era (as Autoplay's Era).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum RadioBy {
    #[default]
    Track,
    Artist,
    Era,
}

impl RadioBy {
    pub fn is_track(&self) -> bool {
        *self == Self::Track
    }
}

/// How many tracks Radio inserts after each own track, unless set; and the most it inserts.
pub const RADIO_COUNT: u8 = 5;
pub const RADIO_MAX: u8 = 10;

/// Autoplay as the queue keeps it (`Queue::autoplay`): on while there is one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
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
    /// A crate it plays inside (its id): only the crate's tracks, as its favourites toggle is set ("Play as radio").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub within: Option<String>,
}

/// What a player asks for.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NextRequest {
    #[serde(default)]
    pub mode: Mode,
    /// Autoplay's seed, else the queue's own tracks; for Radio's inserts, the own track they are like.
    #[serde(default)]
    pub seed: Vec<String>,
    /// Every track already in the queue.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// The playing track.
    #[serde(default)]
    pub last: Option<String>,
    /// How many (about five; whole albums give one album).
    #[serde(default)]
    pub count: Option<usize>,
    /// A crate to stay inside (its id): only its tracks; endless, a new round once all have played.
    #[serde(default)]
    pub within: Option<String>,
    /// Whole albums instead of tracks (Autoplay's `albums`).
    #[serde(default)]
    pub albums: bool,
    /// Radio's inserts: what they are like; left out, the track.
    #[serde(default)]
    pub by: RadioBy,
    /// Leave the listener's favourites out: only music not hearted yet (the More music sheet's switch).
    #[serde(default)]
    pub skip_favourites: bool,
    /// What makes music alike, the first counting most (`Weights::ranked`); left out, the listener's own order (their
    /// settings' `similar_by`, the More music sheet's "Similar by"), else all equally.
    #[serde(default)]
    pub similar_by: Option<Vec<Signal>>,
    /// Whose More music choice applies (0.9.5): a player Cratarr drives passes its account; left out, the listener's.
    #[serde(skip)]
    pub account: Option<String>,
    /// The catalogue folders to choose from (0.9.5), as that choice says; none: every library. Seeds from others still
    /// seed.
    #[serde(skip)]
    pub folders: Option<Vec<i64>>,
}

impl NextRequest {
    /// How much each signal counts for this request: its order, else all equally, the artist double for Radio's
    /// inserts (mostly the same artist, as Symfonium's).
    fn weights(&self) -> Weights {
        match self.similar_by.as_deref() {
            Some(order) if !order.is_empty() => Weights::ranked(order),
            _ if self.mode == Mode::Radio => Weights::EQUAL.with(Signal::Artists, 2.0),
            _ => Weights::EQUAL,
        }
    }
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

const DEFAULT_COUNT: usize = 5;
const MAX_COUNT: usize = 50;
/// Played within this long: left out while there is enough else (the owner, 2026-10-16).
const RECENT: Duration = Duration::from_secs(7 * 24 * 3600);
/// Similar picks among this many of the best.
const TOP: usize = 30;
/// How sharply Similar prefers the closest: a track 0.1 closer is about 4.5 times as likely.
const SHARP: f64 = 15.0;
/// Radio's inserts keep closer still: mostly the track's own artist, as Symfonium's (0.1 closer: about 20 times as likely).
const SHARP_RADIO: f64 = 30.0;
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
    /// What makes it like others (`crate::similarity`): its playlists, artists and album artist, genres, album tags,
    /// year, BPM and what AudioMuse heard.
    pub like: Track,
    /// Its length in seconds, when the library knows it (the queue sheet shows it).
    pub duration_secs: Option<f64>,
    /// Hearted by the listener (`ready`).
    pub favourite: bool,
    /// Excluded by the listener from every automatic pick (`ready`; decision 0024, amended 2026-10-19).
    pub excluded: bool,
    /// Its catalogue folder: its library (Radio and Autoplay's choice of libraries, 0.9.5).
    pub folder: i64,
}

/// The library, ready to choose from.
#[derive(Debug, Clone, Default)]
pub(crate) struct Index {
    pub songs: Vec<Song>,
    by_id: HashMap<String, usize>,
    /// Each album's songs in their order.
    albums: HashMap<u32, Vec<usize>>,
    /// The artists' ids (`ar-…`) by their number, for OpenSubsonic's similar artists.
    artist_ids: Vec<String>,
    /// The genres' names (lower case) by their number: Bandcamp's suggestions are weighed by them (0.9.5).
    genre_ids: Vec<String>,
    /// The songs as similarity's library, by their places (their sounds too); shared, as the index is copied for each
    /// listener.
    like: Arc<Library>,
}

impl Index {
    pub(crate) fn new(songs: Vec<Song>) -> Self {
        let by_id = songs.iter().enumerate().map(|(i, s)| (s.id.clone(), i)).collect();
        let mut albums: HashMap<u32, Vec<usize>> = HashMap::new();
        for (i, s) in songs.iter().enumerate() {
            albums.entry(s.album_n).or_default().push(i);
        }
        let like = Arc::new(Library::new(songs.iter().map(|s| &s.like), Vec::new()));
        Self { songs, by_id, albums, artist_ids: Vec::new(), genre_ids: Vec::new(), like }
    }

    /// With what AudioMuse heard of each song (decision 0054), by its id: its numbers and style tags into the song's
    /// facts, its "sounds like" vector into the library.
    #[cfg(test)]
    pub(crate) fn with_heard(mut self, heard: impl Fn(&str) -> Option<Heard>) -> Self {
        let mut styles = Names::default();
        let mut sounds = Vec::with_capacity(self.songs.len());
        for s in &mut self.songs {
            let h = heard(&s.id).unwrap_or_default();
            (s.like.energy, s.like.danceability, s.like.moods) = (h.energy, h.danceability, h.moods);
            s.like.styles = h.styles.iter().map(|x| styles.of(x)).collect();
            sounds.push(h.sound);
        }
        self.like = Arc::new(Library::new(self.songs.iter().map(|s| &s.like), sounds));
        self
    }

    /// With the catalogue's facts of each song (`crates::Facts::like`: the one reading of a track the crates use too):
    /// its genres, AudioMuse's styles, energy, danceability and moods, its year and tempo, and its sound into the
    /// library. Its artists and playlists stay the index's own; its tags are the facts' (the library's words, 0.9.7, the
    /// household's album tags among them). A song the catalogue has no facts of keeps its year, tempo and album tags and
    /// loses its genres (their numbers are the facts' now).
    pub(crate) fn with_facts(mut self, facts: &[crate::crates::Facts]) -> Self {
        let by_track: HashMap<i64, &crate::crates::Facts> = facts.iter().filter_map(|f| Some((f.track?, f))).collect();
        let mut names: [HashMap<String, u32>; 5] = Default::default();
        let mut intern = |kind: usize, name: &str| {
            let key = name.trim().to_lowercase();
            let next = names[kind].len() as u32;
            *names[kind].entry(key).or_insert(next)
        };
        let mut sounds = Vec::with_capacity(self.songs.len());
        for s in &mut self.songs {
            let f = crate::serve::ids::track_of(&s.id).and_then(|t| by_track.get(&t).copied());
            match f {
                Some(f) => {
                    let mut t = f.like(&mut intern);
                    t.playlists = std::mem::take(&mut s.like.playlists);
                    t.artists = std::mem::take(&mut s.like.artists);
                    s.like = t;
                }
                None => s.like.genres.clear(),
            }
            sounds.push(f.and_then(|f| f.sound.as_deref().map(<[f32]>::to_vec)));
        }
        self.like = Arc::new(Library::new(self.songs.iter().map(|s| &s.like), sounds));
        self
    }

    /// What a set of songs is like.
    fn profile(&self, set: &[usize]) -> Profile {
        Profile::of(&self.like, set, |i| &self.songs[i].like)
    }

    /// An artist's number, by their id (`ar-…`).
    fn artist_number(&self, id: &str) -> Option<u32> {
        self.artist_ids.iter().position(|x| x == id).map(|n| n as u32)
    }

    /// A genre's number, by its name.
    fn genre_number(&self, name: &str) -> Option<u32> {
        let name = name.to_lowercase();
        self.genre_ids.iter().position(|x| *x == name).map(|n| n as u32)
    }

    /// How alike a song is to a set's profile, by the weights.
    fn score(&self, profile: &Profile, i: usize, weights: &Weights) -> f64 {
        profile.to_track(&self.like, i, &self.songs[i].like).score(weights)
    }

    /// The songs an OpenSubsonic id stands for: a song, an album's songs, or an artist's.
    fn of_item(&self, item: &str) -> Vec<usize> {
        if let Some(&i) = self.by_id.get(item) {
            return vec![i];
        }
        if let Some(a) = self.artist_ids.iter().position(|x| x == item) {
            return (0..self.songs.len()).filter(|&i| self.songs[i].like.artists.contains(&(a as u32))).collect();
        }
        (0..self.songs.len()).filter(|&i| self.songs[i].album_id == item).collect()
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

/// What AudioMuse heard of a song (decision 0054), as similarity takes it. The tests' own: nothing else uses it (no
/// warning in the server's build).
#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub(crate) struct Heard {
    pub energy: Option<f32>,
    pub danceability: Option<f32>,
    pub moods: Option<[f32; crate::similarity::MOODS.len()]>,
    pub styles: Vec<String>,
    pub sound: Option<Vec<f32>>,
}


/// The radio's choice for one request: songs of the index, in the order they should play. `recent`: songs played
/// within seven days, with when (epoch seconds). `within`: the crate's songs when it plays inside one.
pub(crate) fn choose(index: &Index, req: &NextRequest, recent: &HashMap<String, i64>, within: Option<&[usize]>, rng: &mut Rng) -> Vec<usize> {
    // Radio by artist or era: Autoplay's Artist or Era, seeded by the one track (decision 0036, amended).
    let as_mode = |mode| NextRequest { mode, by: RadioBy::Track, ..req.clone() };
    let changed = match (req.mode, req.by) {
        (Mode::Radio, RadioBy::Artist) => Some(as_mode(Mode::Artist)),
        (Mode::Radio, RadioBy::Era) => Some(as_mode(Mode::Era)),
        _ => None,
    };
    let req = changed.as_ref().unwrap_or(req);
    let count = req.count.unwrap_or(DEFAULT_COUNT).clamp(1, MAX_COUNT);
    let seed = index.find(&req.seed);
    let last = req.last.as_ref().and_then(|id| index.by_id.get(id).copied());
    if req.mode == Mode::Reshuffle {
        return reshuffle(&seed, last, rng);
    }
    let mut out: HashSet<usize> = index.find(&req.exclude).into_iter().collect();
    // Started from an item: the item's own songs are left out (decision 0036, rule 8). Inside a crate they may play.
    if within.is_none() {
        out.extend(seed.iter().copied());
    }
    out.extend(last);
    let mut all: Vec<usize> = within.map_or_else(|| (0..index.songs.len()).collect(), <[usize]>::to_vec);
    // Only the libraries chosen for Radio and Autoplay (0.9.5); a crate keeps its own.
    if within.is_none()
        && let Some(folders) = &req.folders
    {
        all.retain(|&i| folders.contains(&index.songs[i].folder));
    }
    if req.skip_favourites {
        all.retain(|&i| !index.songs[i].favourite);
    }
    // Never what the listener excluded (decision 0024, amended 2026-10-19: global exclusions).
    all.retain(|&i| !index.songs[i].excluded);
    // Not in the queue; the recently played only when there is not enough else.
    let fresh: Vec<usize> = all.iter().copied().filter(|i| !out.contains(i) && !recent.contains_key(&index.songs[*i].id)).collect();
    // Not enough else: as many of the recently played as are missing come back, those played longest ago first.
    let mut pool = if fresh.len() >= count {
        fresh
    } else {
        let mut stale: Vec<usize> = all.iter().copied().filter(|i| !out.contains(i) && recent.contains_key(&index.songs[*i].id)).collect();
        stale.sort_by_key(|i| recent.get(&index.songs[*i].id).copied().unwrap_or(0));
        let need = count - fresh.len();
        fresh.into_iter().chain(stale.into_iter().take(need)).collect()
    };
    // Inside a crate, all of it played: a new round, those queued longest ago first, not the last few again.
    if within.is_some() && pool.len() < count {
        let queued = index.find(&req.exclude);
        let recent_end = queued.len().saturating_sub(count * 2);
        let mut again: Vec<usize> = Vec::new();
        for &i in &queued[..recent_end] {
            if all.contains(&i) && Some(i) != last && !again.contains(&i) && !pool.contains(&i) {
                again.push(i);
            }
        }
        pool.extend(again.into_iter().take(count - pool.len()));
    }
    // Whole albums (Autoplay's `albums`, Album mix before the amendment): one album, within the mode's limits.
    if req.albums || req.mode == Mode::AlbumMix {
        let narrowed: Vec<usize> = match req.mode {
            Mode::Era => {
                let center = last.and_then(|i| index.songs[i].like.year).or_else(|| index.profile(&seed).year());
                pool.iter().copied().filter(|&i| matches!((center, index.songs[i].like.year), (Some(c), Some(y)) if c.abs_diff(y) <= 5)).collect()
            }
            Mode::Artist => {
                let wanted = wanted_artists(index, &seed, last);
                pool.iter().copied().filter(|&i| index.songs[i].like.artists.iter().any(|a| wanted.contains(a))).collect()
            }
            _ => Vec::new(),
        };
        let from = if narrowed.is_empty() { &pool } else { &narrowed };
        return album_mix(index, from, &seed, &out, req.mode == Mode::Random, &req.weights(), rng);
    }
    let picked = by_mode(index, req, &pool, &seed, &out, last, count, rng);
    // Era and Artist can run dry (no year, artists with nothing more here): Similar fills up the rest.
    if picked.len() < count && matches!(req.mode, Mode::Era | Mode::Artist) {
        let rest: Vec<usize> = pool.iter().copied().filter(|i| !picked.contains(i)).collect();
        let similar = NextRequest { mode: Mode::Similar, count: Some(count - picked.len()), ..req.clone() };
        let more = by_mode(index, &similar, &rest, &seed, &out, picked.last().copied().or(last), count - picked.len(), rng);
        return picked.into_iter().chain(more).collect();
    }
    picked
}

#[allow(clippy::too_many_arguments)]
fn by_mode(index: &Index, req: &NextRequest, pool: &[usize], seed: &[usize], out: &HashSet<usize>, last: Option<usize>, count: usize, rng: &mut Rng) -> Vec<usize> {
    let artists_of = |i: usize| index.songs[i].like.artists.clone();
    match req.mode {
        Mode::Similar | Mode::Continuous | Mode::Radio => {
            let base = if req.mode == Mode::Continuous { last.into_iter().collect() } else { seed.to_vec() };
            let base = if base.is_empty() { last.into_iter().collect() } else { base };
            let profile = index.profile(&base);
            let radio = req.mode == Mode::Radio;
            let weights = req.weights();
            let mut scored: Vec<(usize, f64)> = pool
                .iter()
                .map(|&i| {
                    let s = &index.songs[i];
                    (i, index.score(&profile, i, &weights) + if s.favourite { 0.05 } else { 0.0 } + rng.unit() * 0.1)
                })
                .collect();
            scored.sort_by(|a, b| b.1.total_cmp(&a.1));
            scored.truncate(TOP.max(count * 3));
            // Weighed sharply by closeness: a clearly closer track is much likelier than many nearly as close ones, the
            // others still possible (a plain score as the weight let twenty decent tracks outweigh the best few).
            let top = scored.first().map_or(0.0, |x| x.1);
            let sharp = if radio { SHARP_RADIO } else { SHARP };
            let scored: Vec<(usize, f64)> = scored.into_iter().map(|(i, x)| (i, (sharp * (x - top)).exp())).collect();
            // Radio's inserts may keep to the artist (only the albums are spaced); otherwise never one artist twice in a row.
            pick_spaced(index, scored, count, last.map(artists_of), !radio, rng)
        }
        Mode::AlbumMix => album_mix(index, pool, seed, out, false, &req.weights(), rng),
        Mode::Era => {
            let center = last.and_then(|i| index.songs[i].like.year).or_else(|| index.profile(seed).year());
            let near: Vec<(usize, f64)> = pool
                .iter()
                .filter(|&&i| match (center, index.songs[i].like.year) {
                    (Some(c), Some(y)) => c.abs_diff(y) <= 5,
                    _ => false,
                })
                .map(|&i| (i, rng.unit()))
                .collect();
            pick_spaced(index, near, count, last.map(artists_of), true, rng)
        }
        Mode::Artist => {
            let wanted = wanted_artists(index, seed, last);
            let theirs: Vec<(usize, f64)> =
                pool.iter().filter(|&&i| index.songs[i].like.artists.iter().any(|a| wanted.contains(a))).map(|&i| (i, rng.unit())).collect();
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

/// The playing track's artists and the seed's three most common.
fn wanted_artists(index: &Index, seed: &[usize], last: Option<usize>) -> HashSet<u32> {
    let mut wanted: HashSet<u32> = last.map(|i| index.songs[i].like.artists.clone()).unwrap_or_default().into_iter().collect();
    let mut common: Vec<(u32, u32)> = index.profile(seed).artists().iter().map(|(&a, &n)| (a, n.round() as u32)).collect();
    common.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    wanted.extend(common.into_iter().take(3).map(|(a, _)| a));
    wanted
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
            !albums.contains(&s.album_n) && (!vary_artist || !s.like.artists.iter().any(|a| prev.contains(a)))
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
        prev = index.songs[i].like.artists.clone();
        picked.push(i);
    }
    picked
}

/// One whole album, the closest to the seed (`random`: any) of those with nothing in the queue and none of the seed's own.
fn album_mix(index: &Index, pool: &[usize], seed: &[usize], out: &HashSet<usize>, random: bool, weights: &Weights, rng: &mut Rng) -> Vec<usize> {
    let profile = index.profile(seed);
    let banned: HashSet<u32> = out.iter().map(|&i| index.songs[i].album_n).collect();
    let mut sums: HashMap<u32, (f64, u32)> = HashMap::new();
    for &i in pool {
        let s = &index.songs[i];
        if banned.contains(&s.album_n) {
            continue;
        }
        let e = sums.entry(s.album_n).or_default();
        e.0 += if random { rng.unit() } else { index.score(&profile, i, weights) };
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
    index.albums.get(&album).map(|songs| songs.iter().copied().filter(|i| !out.contains(i) && !index.songs[*i].excluded).collect()).unwrap_or_default()
}

// --- the index, from the library's and the user data's own files ---

struct Cache {
    index: Arc<Index>,
    built_from: String,
    at: Instant,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// The next radio tracks for a player (decision 0036).
pub async fn next(mut req: NextRequest) -> Result<Vec<QueueTrack>, ToggleError> {
    let (index, recent) = ready().await?;
    if req.similar_by.is_none() {
        req.similar_by = similar_by_of_listener();
    }
    // The libraries the account lets Radio and Autoplay choose from (0.9.5): their folders; and whether Bandcamp joins.
    let account = req.account.clone().or_else(|| crate::serve::instance()?.current_user());
    let (libraries, bandcamp) = account.as_deref().map(more_music_of).unwrap_or((None, false));
    if let (Some(libraries), Some(served)) = (libraries, crate::serve::instance()) {
        let folders: Vec<i64> =
            served.library.roots().into_iter().filter(|(_, r)| libraries.iter().any(|l| Path::new(l) == Path::new(r))).map(|(f, _)| f).collect();
        if folders.is_empty() {
            return Err(ToggleError::Refused("No library is switched on for Radio and Autoplay.".into()));
        }
        req.folders = Some(folders);
    }
    let within = match &req.within {
        Some(id) => Some(crate_songs(&index, id).await?),
        None => None,
    };
    if within.as_ref().is_some_and(Vec::is_empty) {
        return Err(ToggleError::Refused("That crate has no tracks of Cratarr's own library.".into()));
    }
    // Radio's inserts in the radio lane; everything else is Autoplay's.
    let lane = if req.mode == Mode::Radio { LANE_RADIO } else { LANE_AUTOPLAY };
    let picks = choose(&index, &req, &recent, within.as_deref(), &mut Rng::new());
    let tracks: Vec<QueueTrack> = picks.iter().map(|&i| track(&index.songs[i], lane)).collect();
    // Bandcamp's suggestions join where the account lets them, on likeness alone (0.9.5; not inside a crate).
    match account {
        Some(account) if bandcamp && within.is_none() && matches!(req.mode, Mode::Similar | Mode::Radio | Mode::Continuous) => {
            Ok(with_elsewhere(&index, &req, &picks, tracks, &account, lane).await)
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
fn elsewhere_of(index: &Index, found: &[super::discover::Suggestion], exclude: &[String]) -> Vec<Elsewhere> {
    let queued: HashSet<String> = exclude.iter().filter_map(|id| crate::bandcamp::read_id(id).map(|(page, _)| page)).collect();
    found
        .iter()
        .filter_map(|s| {
            let page = s.bandcamp.clone().filter(|p| !queued.contains(p))?;
            let names = std::iter::once(s.artist.as_str())
                .chain(s.alike.iter().map(|a| a.seed.as_str()))
                .chain(s.why.iter().filter_map(|w| w.strip_prefix("Like ")));
            let mut artists: Vec<u32> = names.filter_map(|n| index.artist_number(&crate::serve::ids::artist(n))).collect();
            artists.dedup();
            let genres = s.genres.iter().filter_map(|g| index.genre_number(g)).collect();
            let year = s.year.and_then(|y| u32::try_from(y).ok()).filter(|y| *y > 0);
            Some(Elsewhere { page, like: Track { artists, genres, year, ..Track::default() } })
        })
        .collect()
}

/// The places Bandcamp's suggestions take among the picks (0.9.5): each, the most alike first, takes the place of the
/// least alike pick it is more alike than; none where it is not more alike than any. (place, suggestion) pairs.
fn mix_elsewhere(index: &Index, req: &NextRequest, picks: &[usize], elsewhere: &[Elsewhere]) -> Vec<(usize, usize)> {
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
async fn with_elsewhere(index: &Index, req: &NextRequest, picks: &[usize], mut tracks: Vec<QueueTrack>, account: &str, lane: &str) -> Vec<QueueTrack> {
    let Some(served) = crate::serve::instance() else { return tracks };
    let (s, a) = (served.clone(), account.to_string());
    let found = match tokio::task::spawn_blocking(move || super::discover::kept(&s, &a)).await {
        Ok(Ok(Some(f))) => f.albums,
        _ => return tracks,
    };
    let elsewhere = elsewhere_of(index, &found, &req.exclude);
    let mut rng = Rng::new();
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

/// A crate's songs in the index, as its favourites toggle is set (decision 0024), by Cratarr's own ids.
async fn crate_songs(index: &Index, crate_id: &str) -> Result<Vec<usize>, ToggleError> {
    let served = crate::serve::instance().ok_or_else(|| ToggleError::Refused("Cratarr's own library is not open.".into()))?;
    let playlists = served.playlists().ok_or_else(|| ToggleError::Refused("Crates are not ready yet.".into()))?.clone();
    let page = playlists.crate_rows(crate_id, Some(crate::source::cratarr::KIND.id), 0, usize::MAX, &super::CrateLook::default()).await?;
    let ids: Vec<String> = page.rows.into_iter().filter_map(|r| r.id).collect();
    Ok(index.find(&ids))
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
    let since = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0) - RECENT.as_secs() as i64;
    let mut st = conn.prepare("SELECT item, own_date FROM annotation WHERE user = ?1 AND kind = 'song' AND own_date >= ?2")?;
    let rows = st.query_map(params![owner, since], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
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
    let mut st = lib.prepare(
        // A compilation's album artist ("Various Artists") is no artist: it would make every compilation alike.
        "SELECT s.id, s.title, s.artist, s.album, s.album_id, s.year, s.genre, a.genres, s.duration_ms, s.bpm,
                CASE WHEN a.compilation = 1 OR lower(a.artist) IN ('various artists', 'various', 'va', 'verschiedene interpreten')
                     THEN NULL ELSE a.artist_id END, s.folder
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
            folder,
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
    let req = NextRequest { mode: Mode::Similar, exclude: seed.clone(), seed, last: None, count: Some(count.min(MAX_COUNT)), within: None, albums: false, by: RadioBy::Track, skip_favourites: false, similar_by: similar_by_of_listener(), account: None, folders: None };
    Ok(choose(&index, &req, &recent, None, &mut Rng::new()).into_iter().map(|i| index.songs[i].id.clone()).collect())
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
    let req = NextRequest { mode: Mode::Similar, exclude: seed.clone(), seed, last: None, count: Some(MAX_COUNT), within: None, albums: false, by: RadioBy::Track, skip_favourites: false, similar_by: similar_by_of_listener(), account: None, folders: None };
    let mut tally: HashMap<u32, u32> = HashMap::new();
    for i in choose(&index, &req, &recent, None, &mut Rng::new()) {
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
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
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

    fn ids(index: &Index, picked: &[usize]) -> Vec<String> {
        picked.iter().map(|&i| index.songs[i].id.clone()).collect()
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
    fn radio_chooses_only_from_the_libraries_switched_on_and_seeds_still_seed() {
        let mut index = library();
        for s in index.songs.iter_mut().filter(|s| s.id.starts_with('x')) {
            s.folder = 1;
        }
        let only = NextRequest { folders: Some(vec![1]), count: Some(5), ..ask(Mode::Similar) };
        let picked = ids(&index, &choose(&index, &only, &HashMap::new(), None, &mut Rng::seeded(3)));
        assert_eq!(picked.len(), 5);
        assert!(picked.iter().all(|id| id.starts_with('x')), "{picked:?}");
        index.songs.iter_mut().for_each(|s| s.folder = 0);
        let none = NextRequest { folders: Some(vec![1]), ..ask(Mode::Similar) };
        assert!(choose(&index, &none, &HashMap::new(), None, &mut Rng::seeded(3)).is_empty());
        let s = serde_json::json!({ "more_music": { "libraries": ["/media/Music"], "bandcamp": true } });
        assert_eq!(more_music_in(&s), (Some(vec!["/media/Music".to_string()]), true));
        assert_eq!(more_music_in(&serde_json::json!({})), (None, false), "all libraries, no Bandcamp, by default");
    }

    fn ask(mode: Mode) -> NextRequest {
        NextRequest { mode, seed: vec!["s1".into(), "s2".into()], exclude: vec!["s1".into(), "s2".into()], last: Some("s2".into()), count: Some(2), within: None, albums: false, by: RadioBy::Track, skip_favourites: false, similar_by: None, account: None, folders: None }
    }

    #[test]
    fn similar_puts_the_kin_first_and_leaves_out_the_seed() {
        let index = library();
        let picked = ids(&index, &choose(&index, &ask(Mode::Similar), &HashMap::new(), None, &mut Rng::seeded(7)));
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
            let picked = ids(&index, &choose(&index, &ask(Mode::Similar), &recent, None, &mut Rng::seeded(n)));
            assert!(!picked.iter().any(|p| p.starts_with('k')), "{picked:?}");
        }
        // Nothing else left: the recently played come back, the longest ago first.
        let tiny = Index::new(vec![song("s1", 1, 1, 1, 1990, &[]), song("k1", 2, 1, 1, 1990, &[]), song("k2", 3, 1, 1, 1990, &[])]);
        let req = NextRequest { count: Some(1), ..ask(Mode::Random) };
        assert_eq!(ids(&tiny, &choose(&tiny, &NextRequest { seed: vec!["s1".into()], exclude: vec!["s1".into()], ..req }, &recent, None, &mut Rng::seeded(3))), ["k2"]);
    }

    #[test]
    fn spacing_keeps_albums_and_artists_apart() {
        let index = library();
        for n in 0..20 {
            let picked = choose(&index, &NextRequest { count: Some(5), ..ask(Mode::Random) }, &HashMap::new(), None, &mut Rng::seeded(n));
            let albums: HashSet<u32> = picked.iter().map(|&i| index.songs[i].album_n).collect();
            assert_eq!(albums.len(), picked.len(), "two of one album: {:?}", ids(&index, &picked));
        }
    }

    #[test]
    fn album_mix_gives_one_whole_album_not_the_seeds() {
        let index = library();
        let picked = ids(&index, &choose(&index, &ask(Mode::AlbumMix), &HashMap::new(), None, &mut Rng::seeded(1)));
        assert_eq!(picked, ["k1", "k2"], "the closest album, whole and in order");
    }

    #[test]
    fn reshuffle_plays_the_seed_again_never_starting_with_the_last() {
        let index = Index::new((0..6).map(|n| song(&format!("s{n}"), 1, 1, 1, 1990, &[])).collect());
        let seed: Vec<String> = (0..6).map(|n| format!("s{n}")).collect();
        for n in 0..30 {
            let req = NextRequest { mode: Mode::Reshuffle, seed: seed.clone(), exclude: seed.clone(), last: Some("s3".into()), count: None, within: None, albums: false, by: RadioBy::Track, skip_favourites: false, similar_by: None, account: None, folders: None };
            let picked = ids(&index, &choose(&index, &req, &HashMap::new(), None, &mut Rng::seeded(n)));
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
        let req = NextRequest { mode: Mode::Era, seed: vec![], exclude: vec!["now".into()], last: Some("now".into()), count: Some(5), within: None, albums: false, by: RadioBy::Track, skip_favourites: false, similar_by: None, account: None, folders: None };
        let picked = choose(&index, &req, &HashMap::new(), None, &mut Rng::seeded(5));
        assert_eq!(picked.len(), 5);
        assert!(picked.iter().all(|&i| index.songs[i].like.year.unwrap().abs_diff(1990) <= 5));
    }

    #[test]
    fn artist_keeps_to_the_artists() {
        let index = library();
        let req = NextRequest { mode: Mode::Artist, count: Some(5), ..ask(Mode::Artist) };
        let picked = ids(&index, &choose(&index, &req, &HashMap::new(), None, &mut Rng::seeded(2)));
        assert_eq!(picked[0], "k1", "only k1 shares artist 1; k2 is artist 2: {picked:?}");
        assert_eq!(picked.len(), 5, "the artists ran dry: Similar fills up the rest: {picked:?}");
    }

    #[test]
    fn inside_a_crate_only_its_tracks_play_and_a_new_round_follows() {
        let index = library();
        // The crate: s1, s2 (the seed's own album: allowed inside a crate), k1 and three strangers.
        let crate_: Vec<usize> = ["s1", "s2", "k1", "x1", "x2", "x3"].iter().map(|id| index.by_id[*id]).collect();
        let req = NextRequest { mode: Mode::Similar, seed: vec![], exclude: vec!["x1".into()], last: Some("x1".into()), count: Some(5), within: None, albums: false, by: RadioBy::Track, skip_favourites: false, similar_by: None, account: None, folders: None };
        let picked = ids(&index, &choose(&index, &req, &HashMap::new(), Some(&crate_), &mut Rng::seeded(4)));
        assert_eq!(picked.len(), 5, "{picked:?}");
        assert!(picked.iter().all(|p| ["s1", "s2", "k1", "x2", "x3"].contains(&p.as_str())), "only the crate's: {picked:?}");
        // All of it queued: a new round, the longest ago first, not the last few again.
        let queued: Vec<String> = ["s1", "s2", "k1", "x2", "x3", "x1"].iter().map(|s| s.to_string()).collect();
        let req = NextRequest { mode: Mode::Random, seed: vec![], exclude: queued, last: Some("x1".into()), count: Some(2), within: None, albums: false, by: RadioBy::Track, skip_favourites: false, similar_by: None, account: None, folders: None };
        let picked = ids(&index, &choose(&index, &req, &HashMap::new(), Some(&crate_), &mut Rng::seeded(9)));
        assert_eq!(picked.len(), 2, "{picked:?}");
        assert!(picked.iter().all(|p| ["s1", "s2"].contains(&p.as_str())), "the round starts again from the oldest: {picked:?}");
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
            let req = NextRequest { mode: Mode::Radio, seed: one.clone(), exclude: seed.clone(), last: Some(one[0].clone()), count: Some(5), within: None, albums: false, by: RadioBy::Track, skip_favourites: false, similar_by: None, account: None, folders: None };
            println!("-- Radio's inserts after {}", show(album[0]));
            for i in choose(&index, &req, &recent, None, &mut rng) {
                println!("   {}", show(i));
            }
            for mode in [Mode::Similar, Mode::Era, Mode::Artist, Mode::Random] {
                let req = NextRequest { mode, seed: seed.clone(), exclude: seed.clone(), last: seed.last().cloned(), count: Some(5), within: None, albums: false, by: RadioBy::Track, skip_favourites: false, similar_by: None, account: None, folders: None };
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
        let ask = |seed: usize, by: RadioBy, skip: bool| NextRequest {
            mode: Mode::Radio,
            seed: vec![index.songs[seed].id.clone()],
            exclude: vec![index.songs[seed].id.clone()],
            last: Some(index.songs[seed].id.clone()),
            count: Some(5),
            within: None,
            albums: false,
            by,
            skip_favourites: skip,
            similar_by: None,
            account: None,
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

    #[test]
    fn radio_inserts_keep_close_to_the_one_track_and_its_artist() {
        // The seed track's artist has three more songs on other albums, in its genre but other decades; strangers share
        // its genre and era. Artist (double) and genre beat genre and era.
        let mut s = vec![song("own", 1, 7, 1, 2000, &[]), song("a1", 2, 7, 1, 1980, &[]), song("a2", 3, 7, 1, 1975, &[]), song("a3", 4, 7, 1, 2020, &[])];
        s.extend((0..20).map(|n| song(&format!("g{n}"), 10 + n, 20 + n, 1, 2000, &[])));
        let index = Index::new(s);
        let req = NextRequest { mode: Mode::Radio, seed: vec!["own".into()], exclude: vec!["own".into()], last: Some("own".into()), count: Some(3), within: None, albums: false, by: RadioBy::Track, skip_favourites: false, similar_by: None, account: None, folders: None };
        let picked = ids(&index, &choose(&index, &req, &HashMap::new(), None, &mut Rng::seeded(11)));
        assert_eq!(picked.len(), 3);
        assert!(!picked.contains(&"own".to_string()));
        // 3/6 against 2/6, plus a nudge of up to 0.1: the artist's songs are the likeliest by far (one artist after another
        // allowed), the picks still vary. Over 20 runs of 3 picks, most are the artist's (all three would be 60).
        let theirs: usize = (0..20)
            .map(|n| ids(&index, &choose(&index, &req, &HashMap::new(), None, &mut Rng::seeded(n))).iter().filter(|p| p.starts_with('a')).count())
            .sum();
        assert!(theirs >= 40, "{theirs} of 60 picks by the track's artist");
    }

    #[test]
    fn radio_by_era_or_artist_keeps_to_the_tracks_years_or_artist() {
        // The track's artist and genre, far off in years; strangers of its years; the artist's other songs.
        let mut s = vec![song("own", 1, 7, 1, 2000, &[])];
        s.extend((0..6).map(|n| song(&format!("old{n}"), 2 + n, 8, 1, 1970, &[])));
        s.extend((0..6).map(|n| song(&format!("near{n}"), 20 + n, 30 + n, 2, 2002, &[])));
        s.extend((0..2).map(|n| song(&format!("own{n}"), 40 + n, 7, 3, 1985, &[])));
        let index = Index::new(s);
        let ask = |by| NextRequest {
            mode: Mode::Radio,
            seed: vec!["own".into()],
            exclude: vec!["own".into()],
            last: Some("own".into()),
            count: Some(3),
            within: None,
            albums: false,
            by,
            skip_favourites: false,
            similar_by: None,
            account: None,
            folders: None,
        };
        let era = ids(&index, &choose(&index, &ask(RadioBy::Era), &HashMap::new(), None, &mut Rng::seeded(4)));
        assert!(era.len() == 3 && era.iter().all(|p| p.starts_with("near")), "{era:?}");
        let artist = ids(&index, &choose(&index, &ask(RadioBy::Artist), &HashMap::new(), None, &mut Rng::seeded(4)));
        assert_eq!(artist.len(), 3);
        assert!(artist[..2].iter().all(|p| p.starts_with("own")), "the artist's two first, then Similar fills up: {artist:?}");
        let q: NextRequest = serde_json::from_str(r#"{"mode":"radio","by":"artist","count":10}"#).unwrap();
        assert_eq!((q.by, q.count), (RadioBy::Artist, Some(10)));
    }

    #[test]
    fn skipping_favourites_leaves_the_hearted_out() {
        let mut s: Vec<Song> = (0..8).map(|n| song(&format!("t{n}"), 1 + n, 10 + n, 1, 2000, &[])).collect();
        for x in &mut s[1..5] {
            x.favourite = true;
        }
        let index = Index::new(s);
        let ask = |skip_favourites| NextRequest {
            mode: Mode::Similar,
            seed: vec!["t0".into()],
            exclude: vec!["t0".into()],
            last: Some("t0".into()),
            count: Some(5),
            within: None,
            albums: false,
            by: RadioBy::Track,
            skip_favourites,
            similar_by: None,
            account: None,
            folders: None,
        };
        let picked = ids(&index, &choose(&index, &ask(true), &HashMap::new(), None, &mut Rng::seeded(3)));
        assert!(picked.len() == 3 && picked.iter().all(|p| ["t5", "t6", "t7"].contains(&p.as_str())), "{picked:?}");
        assert_eq!(choose(&index, &ask(false), &HashMap::new(), None, &mut Rng::seeded(3)).len(), 5);
    }

    #[test]
    fn a_song_that_sounds_like_the_seed_scores_higher_where_the_other_signals_are_alike() {
        let s = vec![song("seed", 1, 1, 1, 2000, &[]), song("near", 2, 9, 9, 2020, &[]), song("far", 3, 9, 9, 2020, &[]), song("unheard", 4, 9, 9, 2020, &[])];
        let vectors: HashMap<String, Vec<f32>> = [
            ("seed".to_string(), vec![1.0, 0.0]),
            ("near".to_string(), vec![0.995, 0.0998]),
            ("far".to_string(), vec![0.0, 1.0]),
        ]
        .into();
        let index = Index::new(s).with_heard(|id| vectors.get(id).map(|v| Heard { sound: Some(v.clone()), ..Heard::default() }));
        let p = index.profile(&[0]);
        let score = |i| index.score(&p, i, &Weights::EQUAL);
        let (near, far, unheard) = (score(1), score(2), score(3));
        // Known: artist, genre and era (none shared), and the sound where both were heard.
        // The near one is all but the same sound (0.995): about 0.99 of it.
        assert!(near > 0.98 / 4.0 && near < 1.0 / 4.0, "only the sound: {near}");
        assert_eq!(far, 0.0, "not alike at all");
        assert_eq!(unheard, 0.0, "the other signals alone, none shared");
        assert!(near > far);
    }

    #[test]
    fn excluded_tracks_are_never_picked_not_even_in_a_whole_album() {
        let mut s: Vec<Song> = (0..8).map(|n| song(&format!("t{n}"), 1 + n, 10 + n, 1, 2000, &[])).collect();
        for x in &mut s[1..5] {
            x.excluded = true;
        }
        let index = Index::new(s);
        let ask = NextRequest {
            mode: Mode::Similar,
            seed: vec!["t0".into()],
            exclude: vec!["t0".into()],
            last: Some("t0".into()),
            count: Some(5),
            within: None,
            albums: false,
            by: RadioBy::Track,
            skip_favourites: false,
            similar_by: None,
            account: None,
            folders: None,
        };
        let picked = ids(&index, &choose(&index, &ask, &HashMap::new(), None, &mut Rng::seeded(3)));
        assert!(picked.len() == 3 && picked.iter().all(|p| ["t5", "t6", "t7"].contains(&p.as_str())), "{picked:?}");
        // A whole album: its excluded track stays out.
        let mut a = vec![song("s1", 1, 1, 1, 2000, &[]), song("s2", 1, 1, 1, 2000, &[]), song("e1", 2, 2, 9, 1996, &[]), song("e2", 2, 2, 9, 1996, &[])];
        a[3].excluded = true;
        let index = Index::new(a);
        let whole = NextRequest { seed: vec!["s1".into(), "s2".into()], exclude: vec!["s1".into(), "s2".into()], last: Some("s2".into()), albums: true, ..ask };
        assert_eq!(ids(&index, &choose(&index, &whole, &HashMap::new(), None, &mut Rng::seeded(2))), ["e1"]);
    }

    #[test]
    fn albums_give_one_whole_album_within_the_modes_limits() {
        // Seed album 1 (2000); album 2 (1995, the closest by era), album 3 (1960, out of Era's five years).
        let mut s = vec![song("s1", 1, 1, 1, 2000, &[]), song("s2", 1, 1, 1, 2000, &[])];
        s.extend([song("e1", 2, 2, 9, 1996, &[]), song("e2", 2, 2, 9, 1996, &[]), song("o1", 3, 1, 1, 1960, &[]), song("o2", 3, 1, 1, 1960, &[])]);
        let index = Index::new(s);
        let req = NextRequest {
            mode: Mode::Era,
            seed: vec!["s1".into(), "s2".into()],
            exclude: vec!["s1".into(), "s2".into()],
            last: Some("s2".into()),
            count: Some(5),
            within: None,
            albums: true,
            by: RadioBy::Track,
            skip_favourites: false,
            similar_by: None,
            account: None,
            folders: None,
        };
        let picked = ids(&index, &choose(&index, &req, &HashMap::new(), None, &mut Rng::seeded(2)));
        assert_eq!(picked, ["e1", "e2"], "the album in the era, whole, though album 3 shares the artist and genre");
    }

    #[test]
    fn autoplay_and_the_request_read_as_players_send_them() {
        let a: Autoplay = serde_json::from_str(r#"{"mode":"artist","albums":true,"seed":["tr-1"]}"#).unwrap();
        assert_eq!(a, Autoplay { mode: AutoMode::Artist, albums: true, restart: false, seed: vec!["tr-1".into()], within: None });
        assert_eq!(serde_json::to_string(&Autoplay::default()).unwrap(), r#"{"mode":"similar","albums":false,"restart":false}"#);
        let q: NextRequest = serde_json::from_str(r#"{"mode":"radio","seed":["a"],"exclude":["a"],"last":"a","count":5}"#).unwrap();
        assert_eq!((q.mode, q.albums), (Mode::Radio, false));
        assert_eq!(from_old(Some(Mode::AlbumMix)), (AutoMode::Similar, true, false));
        assert_eq!(from_old(Some(Mode::Continuous)), (AutoMode::Similar, false, false));
    }
}
