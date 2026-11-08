//! Radio's picker (decisions 0036, amended, and 0101's point 19): which of a library's tracks come next, for Radio's
//! inserts and Autoplay's modes, so that a player that plays on its own picks as the server does. Its modes are named
//! here, and so what a player asks ([`Ask`]); [`choose`] answers.
//!
//! All it reads is handed in: the library as an [`Index`], with the listener's favourites, exclusions, taste and skips
//! marked on its songs; their recent plays, with when; what it stays inside, as places; a generator. Where those come
//! from is the caller's: the server has them from its files, and asks the web for what the picker never hears of.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::crates::Facts;
use crate::ids::track_of;
use crate::similarity::{Library, Profile, Signal, Track, Weights};
use crate::{Lane, Rng};

/// How Cratarr chooses, as a player asks: Radio's inserts, or one of Autoplay's modes. Continuous, AlbumMix and
/// Reshuffle are the modes from before the amendment, still taken from apps not yet updated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", ts(rename = "RadioMode"))]
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
    /// Your taste (decision 0094): what fits the listener's taste best, as Grave digging ranks it.
    Taste,
    /// Before the amendment: close to the playing track only.
    Continuous,
    /// Before the amendment: a whole album at a time (now `albums`).
    AlbumMix,
    /// Before the amendment: the seed again, in a fresh order (now Autoplay's `restart`, which the player does).
    Reshuffle,
}

/// What Radio's inserts are like (decision 0036, amended): the track (its artist weighed double), its artist (as
/// Autoplay's Artist, seeded by the track) or its era (as Autoplay's Era).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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

/// What the picker is asked: how it chooses, and after what. What it stays inside comes apart, as places (`choose`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ask {
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
    /// The catalogue folders to choose from (0.9.5), as that choice says; none: every library. Seeds from others still
    /// seed.
    #[serde(skip)]
    pub folders: Option<Vec<i64>>,
}

impl Ask {
    /// How much each signal counts for this request: its order, else all equally, the artist double for Radio's
    /// inserts (mostly the same artist, as Symfonium's).
    pub fn weights(&self) -> Weights {
        match self.similar_by.as_deref() {
            Some(order) if !order.is_empty() => Weights::ranked(order),
            _ if self.mode == Mode::Radio => Weights::EQUAL.with(Signal::Artists, 2.0),
            _ => Weights::EQUAL,
        }
    }
}

/// How many tracks are picked unless asked, and the most.
pub const DEFAULT_COUNT: usize = 5;
pub const MAX_COUNT: usize = 50;
/// Similar picks among this many of the best.
const TOP: usize = 30;
/// How sharply Similar prefers the closest: a track 0.1 closer is about 4.5 times as likely.
const SHARP: f64 = 15.0;
/// Radio's inserts keep closer still: mostly the track's own artist, as Symfonium's (0.1 closer: about 20 times as likely).
const SHARP_RADIO: f64 = 30.0;
/// How far what is like the listener's skips is kept away (decision 0094: later and less often, never left out): a
/// track as like them as can be about a twentieth as likely as one not like them at all.
const SKIPPED_AWAY: f64 = 3.0;

/// One song as the radio sees it; names interned as numbers.
#[derive(Debug, Clone, Default)]
pub struct Song {
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
    /// Hearted by the listener: the caller marks it, for each choice.
    pub favourite: bool,
    /// Excluded by the listener from every automatic pick (decision 0024, amended 2026-10-19): marked the same way.
    pub excluded: bool,
    /// How well it fits the listener's taste, for Your taste (decision 0094); none: not known.
    pub taste: Option<f64>,
    /// How like it is to what the listener skips, 0 to 1 (decision 0094): it comes later and less often.
    pub skipped: f64,
    /// The catalogue folders it lies in: its libraries (Radio and Autoplay's choice of libraries, 0.9.5).
    pub folders: Vec<i64>,
}

/// The library, ready to choose from.
#[derive(Debug, Clone, Default)]
pub struct Index {
    pub songs: Vec<Song>,
    pub by_id: HashMap<String, usize>,
    /// Each album's songs in their order.
    pub albums: HashMap<u32, Vec<usize>>,
    /// The artists' ids (`ar-…`) by their number, for OpenSubsonic's similar artists.
    pub artist_ids: Vec<String>,
    /// The genres' names (lower case) by their number: Bandcamp's suggestions are weighed by them (0.9.5).
    pub genre_ids: Vec<String>,
    /// The songs as similarity's library, by their places (their sounds too); shared, as the index is copied for each
    /// listener.
    pub like: Arc<Library>,
}

impl Index {
    pub fn new(songs: Vec<Song>) -> Self {
        let by_id = songs.iter().enumerate().map(|(i, s)| (s.id.clone(), i)).collect();
        let mut albums: HashMap<u32, Vec<usize>> = HashMap::new();
        for (i, s) in songs.iter().enumerate() {
            albums.entry(s.album_n).or_default().push(i);
        }
        let like = Arc::new(Library::new(songs.iter().map(|s| &s.like), Vec::new()));
        Self { songs, by_id, albums, artist_ids: Vec::new(), genre_ids: Vec::new(), like }
    }

    /// With the catalogue's facts of each song (`crates::Facts::like`: the one reading of a track the crates use too):
    /// its genres, AudioMuse's styles, energy, danceability and moods, its year and tempo, and its sound into the
    /// library. Its artists and playlists stay the index's own; its tags are the facts' (the library's words, 0.9.7, the
    /// household's album tags among them). A song the catalogue has no facts of keeps its year, tempo and album tags and
    /// loses its genres (their numbers are the facts' now).
    pub fn with_facts(mut self, facts: &[Facts]) -> Self {
        let by_track: HashMap<i64, &Facts> = facts.iter().filter_map(|f| Some((f.track?, f))).collect();
        let mut names: [HashMap<String, u32>; 5] = Default::default();
        let mut intern = |kind: usize, name: &str| {
            let key = name.trim().to_lowercase();
            let next = names[kind].len() as u32;
            *names[kind].entry(key).or_insert(next)
        };
        let mut sounds = Vec::with_capacity(self.songs.len());
        for s in &mut self.songs {
            let f = track_of(&s.id).and_then(|t| by_track.get(&t).copied());
            match f {
                Some(f) => {
                    let mut t = f.like(&mut intern);
                    t.playlists = std::mem::take(&mut s.like.playlists);
                    t.artists = std::mem::take(&mut s.like.artists);
                    s.like = t;
                    if !f.folders.is_empty() {
                        s.folders = f.folders.clone();
                    }
                }
                None => s.like.genres.clear(),
            }
            sounds.push(f.and_then(|f| f.sound.clone()));
        }
        self.like = Arc::new(Library::new(self.songs.iter().map(|s| &s.like), sounds));
        self
    }

    /// What a set of songs is like.
    pub fn profile(&self, set: &[usize]) -> Profile {
        Profile::of(&self.like, set, |i| &self.songs[i].like)
    }

    /// An artist's number, by their id (`ar-…`).
    pub fn artist_number(&self, id: &str) -> Option<u32> {
        self.artist_ids.iter().position(|x| x == id).map(|n| n as u32)
    }

    /// A genre's number, by its name.
    pub fn genre_number(&self, name: &str) -> Option<u32> {
        let name = name.to_lowercase();
        self.genre_ids.iter().position(|x| *x == name).map(|n| n as u32)
    }

    /// How alike a song is to a set's profile, by the weights.
    pub fn score(&self, profile: &Profile, i: usize, weights: &Weights) -> f64 {
        profile.to_track(&self.like, i, &self.songs[i].like).score(weights)
    }

    /// The songs an OpenSubsonic id stands for: a song, an album's songs, or an artist's.
    pub fn of_item(&self, item: &str) -> Vec<usize> {
        if let Some(&i) = self.by_id.get(item) {
            return vec![i];
        }
        if let Some(a) = self.artist_ids.iter().position(|x| x == item) {
            return (0..self.songs.len()).filter(|&i| self.songs[i].like.artists.contains(&(a as u32))).collect();
        }
        (0..self.songs.len()).filter(|&i| self.songs[i].album_id == item).collect()
    }

    pub fn find(&self, ids: &[String]) -> Vec<usize> {
        ids.iter().filter_map(|id| self.by_id.get(id).copied()).collect()
    }
}

/// The radio's choice for one request: songs of the index, in the order they should play. `recent`: songs played
/// within seven days, with when (epoch seconds). `within`: the songs of what it plays inside (a crate, a playlist…).
pub fn choose(index: &Index, req: &Ask, recent: &HashMap<String, i64>, within: Option<&[usize]>, rng: &mut Rng) -> Vec<usize> {
    choose_among(index, req, recent, within, None, rng)
}

/// `choose`, picking only what `here` holds (a device with Cratarr away: what plays there; the owner, 2026-11-06: what cannot play is
/// left out, with no notice). What is not here still seeds, and still counts as played.
pub fn choose_among(index: &Index, req: &Ask, recent: &HashMap<String, i64>, within: Option<&[usize]>, here: Option<&HashSet<usize>>, rng: &mut Rng) -> Vec<usize> {
    // Radio by artist or era: Autoplay's Artist or Era, seeded by the one track (decision 0036, amended).
    let as_mode = |mode| Ask { mode, by: RadioBy::Track, ..req.clone() };
    let changed = match (req.mode, req.by) {
        (Mode::Radio, RadioBy::Artist) => Some(as_mode(Mode::Artist)),
        (Mode::Radio, RadioBy::Era) => Some(as_mode(Mode::Era)),
        // Your taste with no taste yet (no one asks, nothing liked): Similar.
        (Mode::Taste, _) if index.songs.iter().all(|s| s.taste.is_none()) => Some(as_mode(Mode::Similar)),
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
    // Started from an item: the item's own songs are left out (decision 0036, rule 8). Inside what it stays in they
    // may play.
    if within.is_none() {
        out.extend(seed.iter().copied());
    }
    out.extend(last);
    let mut all: Vec<usize> = within.map_or_else(|| (0..index.songs.len()).collect(), <[usize]>::to_vec);
    // Only the libraries chosen for Radio and Autoplay (0.9.5); what it stays inside keeps its own.
    if within.is_none()
        && let Some(folders) = &req.folders
    {
        all.retain(|&i| index.songs[i].folders.iter().any(|f| folders.contains(f)));
    }
    if let Some(here) = here {
        all.retain(|i| here.contains(i));
    }
    if req.skip_favourites {
        all.retain(|&i| !index.songs[i].favourite);
    }
    // Never what the listener excluded (decision 0024, amended 2026-10-19: global exclusions).
    all.retain(|&i| !index.songs[i].excluded);
    // All that may be picked at all: what a mode takes from outside the pool is held to it too.
    let allowed: HashSet<usize> = all.iter().copied().collect();
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
    // Inside it, all of it played: a new round, those queued longest ago first, not the last few again.
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
        // Whole but for what may not be picked: inside a crate only its tracks, and every hearted one out while favourites
        // are ignored (0036), as an excluded one always was.
        return album_mix(index, from, &seed, &out, req.mode, &req.weights(), rng).into_iter().filter(|i| allowed.contains(i)).collect();
    }
    let picked = by_mode(index, req, &pool, &seed, &allowed, last, count, rng);
    // Era and Artist can run dry (no year, artists with nothing more here): Similar fills up the rest.
    if picked.len() < count && matches!(req.mode, Mode::Era | Mode::Artist) {
        let rest: Vec<usize> = pool.iter().copied().filter(|i| !picked.contains(i)).collect();
        let similar = Ask { mode: Mode::Similar, count: Some(count - picked.len()), ..req.clone() };
        let more = by_mode(index, &similar, &rest, &seed, &allowed, picked.last().copied().or(last), count - picked.len(), rng);
        return picked.into_iter().chain(more).collect();
    }
    picked
}

#[allow(clippy::too_many_arguments)]
fn by_mode(index: &Index, req: &Ask, pool: &[usize], seed: &[usize], allowed: &HashSet<usize>, last: Option<usize>, count: usize, rng: &mut Rng) -> Vec<usize> {
    let artists_of = |i: usize| index.songs[i].like.artists.clone();
    match req.mode {
        Mode::Similar | Mode::Continuous | Mode::Radio | Mode::Taste => {
            let base = if req.mode == Mode::Continuous { last.into_iter().collect() } else { seed.to_vec() };
            let base = if base.is_empty() { last.into_iter().collect() } else { base };
            let profile = index.profile(&base);
            let radio = req.mode == Mode::Radio;
            let weights = req.weights();
            // Like the seed, favourites a little closer; Your taste by fit to the taste, where favourites weigh fully
            // (decision 0094).
            let close = |i: usize| {
                let s = &index.songs[i];
                match req.mode {
                    Mode::Taste => s.taste.unwrap_or(0.0),
                    _ => index.score(&profile, i, &weights) + if s.favourite { 0.05 } else { 0.0 },
                }
            };
            let mut scored: Vec<(usize, f64)> = pool.iter().map(|&i| (i, close(i) + rng.unit() * 0.1)).collect();
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
            // From the seed first (its own tracks again, those not in the queue), then from the whole library. The pool left
            // the seed out, so the seed answers to what may be picked by itself: a seed from a library not chosen still seeds,
            // but it is never picked (0036), and what the listener excluded or ignores comes back by no road.
            let from_seed: Vec<(usize, f64)> = seed.iter().copied().filter(|i| allowed.contains(i) && !queued.contains(i) && Some(*i) != last).map(|i| (i, rng.unit())).collect();
            let mut picked = pick_spaced(index, from_seed, count, None, false, rng);
            if picked.len() < count {
                let rest: Vec<(usize, f64)> = pool.iter().filter(|i| !seeds.contains(i) && !picked.contains(i)).map(|&i| (i, rng.unit())).collect();
                picked.extend(pick_spaced(index, rest, count - picked.len(), None, true, rng));
            }
            picked
        }
        Mode::AlbumMix | Mode::Reshuffle => unreachable!("answered above"),
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
        // Weighted by score: the best are likeliest, the others still possible; what is like the listener's skips less
        // so (decision 0094), never not at all.
        let weight = |k: usize| (candidates[k].1 * away(&index.songs[candidates[k].0])).max(0.001);
        let total: f64 = pool.iter().map(|&k| weight(k)).sum();
        let mut at = rng.unit() * total;
        let mut chosen = pool[pool.len() - 1];
        for &k in &pool {
            at -= weight(k);
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

/// How much a song's chance is kept (0 to 1) for being like what the listener skips (`SKIPPED_AWAY`).
fn away(s: &Song) -> f64 {
    (-SKIPPED_AWAY * s.skipped).exp()
}

/// One whole album, the closest to the seed (Random: any; Your taste: the one that fits the taste best) of those with
/// nothing in the queue and none of the seed's own. `choose` holds its tracks to what may be picked.
fn album_mix(index: &Index, pool: &[usize], seed: &[usize], out: &HashSet<usize>, mode: Mode, weights: &Weights, rng: &mut Rng) -> Vec<usize> {
    let profile = index.profile(seed);
    let banned: HashSet<u32> = out.iter().map(|&i| index.songs[i].album_n).collect();
    // In the albums' order, for the draws below: a map's own order differs from run to run, and one seed gives one answer.
    let mut sums: BTreeMap<u32, (f64, u32)> = BTreeMap::new();
    for &i in pool {
        let s = &index.songs[i];
        if banned.contains(&s.album_n) {
            continue;
        }
        let e = sums.entry(s.album_n).or_default();
        let close = match mode {
            Mode::Random => rng.unit(),
            Mode::Taste => s.taste.unwrap_or(0.0),
            _ => index.score(&profile, i, weights),
        };
        // Like what the listener skips: weighed down (decision 0094).
        e.0 += close * away(s);
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

// --- What a player decides of its queue (decision 0036, amended; audit/KOTLIN-DECISIONS.md): when and where, so that no player keeps
// a rule of its own. What it does with the answer is its own: only it knows when a track starts and what is left.

/// Autoplay asks when fewer than this many tracks are left ahead, and appends this many.
pub const AUTOPLAY_LOW: usize = 3;
pub const AUTOPLAY_BATCH: usize = DEFAULT_COUNT;

/// Whether Autoplay asks now: `count` tracks in the queue, the one at `playing` playing.
pub fn autoplay_due(count: usize, playing: usize) -> bool {
    count > 0 && count.saturating_sub(playing + 1) < AUTOPLAY_LOW
}

/// Where Radio's inserts go for the track at `playing`: after it and its run of tracks added by hand. None where they would not: the
/// playing track is not one of the queue's own, or inserts lie there already.
pub fn radio_place(lanes: &[Lane], playing: usize) -> Option<usize> {
    if lanes.get(playing) != Some(&Lane::Own) {
        return None;
    }
    let at = playing + 1 + crate::added_ahead(lanes, Some(playing));
    (lanes.get(at) != Some(&Lane::Radio)).then_some(at)
}

/// A track of a queue, as repeat all reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Queued {
    /// Who it is: `source/id`.
    pub key: String,
    /// Its place in the album or playlist as it was played; none where the queue was played before places were kept.
    pub place: Option<u32>,
    pub lane: Lane,
}

/// The next round of repeat all (decision 0033, amended), as places in `queue`, or none while the track at `playing` is not the last:
/// the album's or playlist's own tracks by their place, the last time each played, those added by hand not again; without places, its
/// own tracks once each in their order. Shuffled anew where it plays shuffled, and never opening on the track that just played.
pub fn next_round(queue: &[Queued], playing: usize, shuffled: bool, rng: &mut Rng) -> Vec<usize> {
    if queue.is_empty() || playing != queue.len() - 1 {
        return Vec::new();
    }
    let own = |t: &Queued| t.lane == Lane::Own;
    let mut by_place: BTreeMap<u32, usize> = BTreeMap::new();
    for (i, t) in queue.iter().enumerate().filter(|(_, t)| own(t)) {
        if let Some(place) = t.place {
            by_place.insert(place, i);
        }
    }
    let mut round: Vec<usize> = if by_place.is_empty() {
        let mut seen = HashSet::new();
        (0..queue.len()).filter(|&i| own(&queue[i]) && seen.insert(queue[i].key.as_str())).collect()
    } else {
        by_place.into_values().collect()
    };
    if shuffled {
        rng.shuffle(&mut round);
        if round.len() > 1 && queue[round[0]].key == queue[playing].key {
            round.rotate_left(1);
        }
    }
    round
}

/// A track kept on a device, for a pick with nothing of the library to go by.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeptTrack {
    pub key: String,
    pub artist: Option<String>,
}

/// What comes next with no index on the device (it never synced, or it has no rules library's room): of the `kept` tracks not in the
/// queue, at most `count`, at random, those by one of `artists` first unless `any`. Places in `kept`.
pub fn from_kept(kept: &[KeptTrack], queued: &[String], artists: &[String], any: bool, count: usize, rng: &mut Rng) -> Vec<usize> {
    let mut places: Vec<usize> = (0..kept.len()).filter(|&i| !queued.contains(&kept[i].key)).collect();
    rng.shuffle(&mut places);
    if !any {
        places.sort_by_key(|&i| !kept[i].artist.as_ref().is_some_and(|a| artists.contains(a)));
    }
    places.truncate(count);
    places
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
            like: Track { artists: vec![artist], genres: vec![genre], year: Some(year), playlists: playlists.to_vec(), ..Track::default() },
            folders: vec![0],
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

    fn ask(mode: Mode) -> Ask {
        Ask { mode, seed: vec!["s1".into(), "s2".into()], exclude: vec!["s1".into(), "s2".into()], last: Some("s2".into()), count: Some(2), albums: false, by: RadioBy::Track, skip_favourites: false, similar_by: None, folders: None }
    }

    impl Index {
        /// With what AudioMuse heard of each song, by its id: the sound alone, as `with_facts` makes it from a track's facts.
        fn with_sounds(mut self, sound: impl Fn(&str) -> Option<Vec<f32>>) -> Self {
            let sounds = self.songs.iter().map(|s| sound(&s.id).map(Arc::from)).collect();
            self.like = Arc::new(Library::new(self.songs.iter().map(|s| &s.like), sounds));
            self
        }
    }

    /// `k1`, `k2` and the strangers from 3 on, those not here left out, whatever the seed.
    fn picked_here(req: &Ask, within: Option<&[usize]>, here: &[&str]) -> Vec<Vec<String>> {
        let index = library();
        let here: HashSet<usize> = here.iter().map(|id| index.by_id[*id]).collect();
        (0..40).map(|n| ids(&index, &choose_among(&index, req, &HashMap::new(), within, Some(&here), &mut Rng::seeded(n)))).collect()
    }

    #[test]
    fn a_pick_among_what_plays_here_holds_only_that_in_every_mode() {
        let here = ["k1", "x1", "x2", "x3", "x4", "x5", "x6"];
        for mode in [Mode::Similar, Mode::Artist, Mode::Era, Mode::Random, Mode::Radio, Mode::Taste] {
            for albums in [false, true] {
                let req = Ask { albums, count: Some(5), ..ask(mode) };
                for picked in picked_here(&req, None, &here) {
                    assert!(picked.iter().all(|p| here.contains(&p.as_str())), "{mode:?}: {picked:?}");
                }
            }
        }
    }

    #[test]
    fn what_is_not_here_still_seeds() {
        // s1 and s2 are not here: what is like them comes first all the same.
        let picks = picked_here(&ask(Mode::Similar), None, &["k1", "x1", "x2", "x3"]);
        let near = picks.iter().filter(|p| p.contains(&"k1".to_string())).count();
        assert!(near >= 30, "the kin, mostly: {near} of 40");
    }

    #[test]
    fn inside_a_crate_a_pick_holds_what_is_of_it_and_here() {
        let index = library();
        let crate_: Vec<usize> = ["k1", "k2", "x1", "x2"].iter().map(|id| index.by_id[*id]).collect();
        let req = Ask { count: Some(5), seed: vec![], exclude: vec![], last: None, ..ask(Mode::Random) };
        for picked in picked_here(&req, Some(&crate_), &["k2", "x2", "x3"]) {
            assert_eq!(picked.iter().map(String::as_str).collect::<HashSet<_>>(), HashSet::from(["k2", "x2"]), "of it and here, nothing else");
        }
    }

    #[test]
    fn with_nothing_said_of_what_is_here_a_pick_is_the_servers() {
        let index = library();
        for n in 0..20 {
            let (a, b) = (choose(&index, &ask(Mode::Similar), &HashMap::new(), None, &mut Rng::seeded(n)), choose_among(&index, &ask(Mode::Similar), &HashMap::new(), None, None, &mut Rng::seeded(n)));
            assert_eq!(a, b);
        }
    }

    #[test]
    fn autoplay_asks_when_fewer_than_three_are_left_ahead() {
        assert!(!autoplay_due(0, 0), "an empty queue asks nothing");
        assert!(autoplay_due(1, 0));
        assert!(autoplay_due(3, 0), "two left");
        assert!(!autoplay_due(4, 0), "three left");
        assert!(autoplay_due(4, 1));
        assert!(autoplay_due(4, 3));
    }

    #[test]
    fn radios_inserts_go_after_the_playing_track_and_its_hand_added_run() {
        use Lane::{Autoplay, Next, Own, Radio};
        assert_eq!(radio_place(&[Own, Own], 0), Some(1));
        assert_eq!(radio_place(&[Own, Next, Next, Own], 0), Some(3));
        assert_eq!(radio_place(&[Own, Next], 0), Some(2), "at the end");
        assert_eq!(radio_place(&[Own, Autoplay], 0), Some(1), "before Autoplay's");
        assert_eq!(radio_place(&[Own, Radio, Own], 0), None, "there already");
        assert_eq!(radio_place(&[Own, Next, Radio], 0), None, "after its run too");
        assert_eq!(radio_place(&[Radio, Own], 0), None, "only the queue's own get inserts");
        assert_eq!(radio_place(&[Next, Own], 0), None);
        assert_eq!(radio_place(&[Own], 3), None, "no such place");
    }

    fn queued(key: &str, place: Option<u32>, lane: Lane) -> Queued {
        Queued { key: key.into(), place, lane }
    }

    #[test]
    fn the_next_round_is_the_albums_own_by_place_those_added_by_hand_not_again() {
        let queue = [queued("c", Some(2), Lane::Own), queued("n", Some(9), Lane::Next), queued("a", Some(0), Lane::Own), queued("r", None, Lane::Radio), queued("b", Some(1), Lane::Own)];
        assert_eq!(next_round(&queue, 4, false, &mut Rng::seeded(1)), [2, 4, 0]);
        assert!(next_round(&queue, 3, false, &mut Rng::seeded(1)).is_empty(), "only while the last plays");
        assert!(next_round(&[], 0, false, &mut Rng::seeded(1)).is_empty());
    }

    #[test]
    fn a_round_again_takes_the_last_time_each_place_played() {
        let queue = [queued("a", Some(0), Lane::Own), queued("b", Some(1), Lane::Own), queued("a", Some(0), Lane::Own)];
        assert_eq!(next_round(&queue, 2, false, &mut Rng::seeded(1)), [2, 1]);
    }

    #[test]
    fn a_queue_without_places_goes_round_its_own_once_each() {
        let queue = [queued("a", None, Lane::Own), queued("b", None, Lane::Own), queued("a", None, Lane::Own), queued("n", None, Lane::Next)];
        assert_eq!(next_round(&queue, 3, false, &mut Rng::seeded(1)), [0, 1]);
    }

    #[test]
    fn a_shuffled_round_never_opens_on_the_track_that_just_played() {
        let queue: Vec<Queued> = (0..6).map(|n| queued(&format!("t{n}"), Some(n), Lane::Own)).collect();
        let mut first = HashSet::new();
        for n in 0..60 {
            let round = next_round(&queue, 5, true, &mut Rng::seeded(n));
            assert_eq!(round.iter().copied().collect::<HashSet<_>>().len(), 6, "every track once");
            assert_ne!(round[0], 5);
            first.insert(round[0]);
        }
        assert!(first.len() > 2, "shuffled anew: {first:?}");
    }

    #[test]
    fn with_no_index_kept_tracks_not_queued_come_their_artists_first() {
        let kept: Vec<KeptTrack> = [("a", "X"), ("b", "Y"), ("c", "X"), ("d", "Z")].map(|(k, a)| KeptTrack { key: k.into(), artist: Some(a.into()) }).to_vec();
        let queued = vec!["c".to_string()];
        for n in 0..20 {
            let picks = from_kept(&kept, &queued, &["Y".to_string()], false, 2, &mut Rng::seeded(n));
            assert_eq!(picks.len(), 2);
            assert_eq!(picks[0], 1, "the artist's first");
            assert!(!picks.contains(&2), "not what is queued");
        }
        let any: HashSet<usize> = (0..40).map(|n| from_kept(&kept, &queued, &["Y".to_string()], true, 1, &mut Rng::seeded(n))[0]).collect();
        assert!(any.len() > 1, "at random: {any:?}");
    }

    #[test]
    fn a_song_is_in_every_library_that_holds_a_copy() {
        let mut index = library();
        for s in index.songs.iter_mut().filter(|s| s.id.starts_with('x')) {
            s.folders = vec![0, 1];
        }
        let only = Ask { folders: Some(vec![1]), count: Some(5), ..ask(Mode::Similar) };
        let picked = ids(&index, &choose(&index, &only, &HashMap::new(), None, &mut Rng::seeded(3)));
        assert_eq!(picked.len(), 5);
        assert!(picked.iter().all(|id| id.starts_with('x')), "{picked:?}");
        let one = Song { id: "tr-1".into(), folders: vec![0], ..Song::default() };
        let told = Index::new(vec![one]).with_facts(&[Facts { track: Some(1), folders: vec![0, 2], ..Facts::default() }]);
        assert_eq!(told.songs[0].folders, [0, 2], "as the catalogue's facts have it");
    }

    #[test]
    fn radio_chooses_only_from_the_libraries_switched_on_and_seeds_still_seed() {
        let mut index = library();
        for s in index.songs.iter_mut().filter(|s| s.id.starts_with('x')) {
            s.folders = vec![1];
        }
        let only = Ask { folders: Some(vec![1]), count: Some(5), ..ask(Mode::Similar) };
        let picked = ids(&index, &choose(&index, &only, &HashMap::new(), None, &mut Rng::seeded(3)));
        assert_eq!(picked.len(), 5);
        assert!(picked.iter().all(|id| id.starts_with('x')), "{picked:?}");
        index.songs.iter_mut().for_each(|s| s.folders = vec![0]);
        let none = Ask { folders: Some(vec![1]), ..ask(Mode::Similar) };
        assert!(choose(&index, &none, &HashMap::new(), None, &mut Rng::seeded(3)).is_empty());
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
        let req = Ask { count: Some(1), ..ask(Mode::Random) };
        assert_eq!(ids(&tiny, &choose(&tiny, &Ask { seed: vec!["s1".into()], exclude: vec!["s1".into()], ..req }, &recent, None, &mut Rng::seeded(3))), ["k2"]);
    }

    #[test]
    fn spacing_keeps_albums_and_artists_apart() {
        let index = library();
        for n in 0..20 {
            let picked = choose(&index, &Ask { count: Some(5), ..ask(Mode::Random) }, &HashMap::new(), None, &mut Rng::seeded(n));
            let albums: HashSet<u32> = picked.iter().map(|&i| index.songs[i].album_n).collect();
            assert_eq!(albums.len(), picked.len(), "two of one album: {:?}", ids(&index, &picked));
        }
    }

    /// Every album alike, so the draw alone decides; a map's order differs from map to map, and the draws must not.
    #[test]
    fn one_seed_gives_one_whole_album_every_time() {
        let index = Index::new((0..40).flat_map(|a| [song(&format!("a{a}"), a, a, 1, 2000, &[]), song(&format!("b{a}"), a, a, 1, 2000, &[])]).collect());
        let req = Ask { mode: Mode::Similar, albums: true, ..Ask::default() };
        let first = choose(&index, &req, &HashMap::new(), None, &mut Rng::seeded(7));
        assert_eq!(first.len(), 2);
        for _ in 0..50 {
            assert_eq!(choose(&index, &req, &HashMap::new(), None, &mut Rng::seeded(7)), first);
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
            let req = Ask { mode: Mode::Reshuffle, seed: seed.clone(), exclude: seed.clone(), last: Some("s3".into()), count: None, albums: false, by: RadioBy::Track, skip_favourites: false, similar_by: None, folders: None };
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
        let req = Ask { mode: Mode::Era, seed: vec![], exclude: vec!["now".into()], last: Some("now".into()), count: Some(5), albums: false, by: RadioBy::Track, skip_favourites: false, similar_by: None, folders: None };
        let picked = choose(&index, &req, &HashMap::new(), None, &mut Rng::seeded(5));
        assert_eq!(picked.len(), 5);
        assert!(picked.iter().all(|&i| index.songs[i].like.year.unwrap().abs_diff(1990) <= 5));
    }

    #[test]
    fn artist_keeps_to_the_artists() {
        let index = library();
        let req = Ask { mode: Mode::Artist, count: Some(5), ..ask(Mode::Artist) };
        let picked = ids(&index, &choose(&index, &req, &HashMap::new(), None, &mut Rng::seeded(2)));
        assert_eq!(picked[0], "k1", "only k1 shares artist 1; k2 is artist 2: {picked:?}");
        assert_eq!(picked.len(), 5, "the artists ran dry: Similar fills up the rest: {picked:?}");
    }

    #[test]
    fn inside_a_crate_only_its_tracks_play_and_a_new_round_follows() {
        let index = library();
        // The crate: s1, s2 (the seed's own album: allowed inside a crate), k1 and three strangers.
        let crate_: Vec<usize> = ["s1", "s2", "k1", "x1", "x2", "x3"].iter().map(|id| index.by_id[*id]).collect();
        let req = Ask { mode: Mode::Similar, seed: vec![], exclude: vec!["x1".into()], last: Some("x1".into()), count: Some(5), albums: false, by: RadioBy::Track, skip_favourites: false, similar_by: None, folders: None };
        let picked = ids(&index, &choose(&index, &req, &HashMap::new(), Some(&crate_), &mut Rng::seeded(4)));
        assert_eq!(picked.len(), 5, "{picked:?}");
        assert!(picked.iter().all(|p| ["s1", "s2", "k1", "x2", "x3"].contains(&p.as_str())), "only the crate's: {picked:?}");
        // All of it queued: a new round, the longest ago first, not the last few again.
        let queued: Vec<String> = ["s1", "s2", "k1", "x2", "x3", "x1"].iter().map(|s| s.to_string()).collect();
        let req = Ask { mode: Mode::Random, seed: vec![], exclude: queued, last: Some("x1".into()), count: Some(2), albums: false, by: RadioBy::Track, skip_favourites: false, similar_by: None, folders: None };
        let picked = ids(&index, &choose(&index, &req, &HashMap::new(), Some(&crate_), &mut Rng::seeded(9)));
        assert_eq!(picked.len(), 2, "{picked:?}");
        assert!(picked.iter().all(|p| ["s1", "s2"].contains(&p.as_str())), "the round starts again from the oldest: {picked:?}");
    }

    #[test]
    fn your_taste_picks_what_fits_the_taste_best_spaced_as_similar() {
        // The taste fits two strangers best, of two artists; the rest a little.
        let mut songs = library().songs;
        for s in songs.iter_mut() {
            s.taste = Some(match s.id.as_str() {
                "x5" => 0.9,
                "x6" => 0.85,
                _ => 0.2,
            });
        }
        songs.iter_mut().filter(|s| s.id == "x6").for_each(|s| s.like.artists = vec![20]);
        let mut index = Index::new(songs);
        let mut best = 0;
        for n in 0..40 {
            let picked = ids(&index, &choose(&index, &ask(Mode::Taste), &HashMap::new(), None, &mut Rng::seeded(n)));
            assert_eq!(picked.len(), 2);
            if picked.contains(&"x5".to_string()) && picked.contains(&"x6".to_string()) {
                best += 1;
            }
        }
        assert!(best >= 30, "the two that fit best, mostly: {best} of 40");
        // Inside a list, only its own.
        let list: Vec<usize> = ["k1", "k2", "x1"].iter().map(|id| index.by_id[*id]).collect();
        let picked = ids(&index, &choose(&index, &ask(Mode::Taste), &HashMap::new(), Some(&list), &mut Rng::seeded(1)));
        assert!(picked.iter().all(|p| ["k1", "k2", "x1"].contains(&p.as_str())), "{picked:?}");
        // No taste yet: Similar.
        index.songs.iter_mut().for_each(|s| s.taste = None);
        let picked = ids(&index, &choose(&index, &ask(Mode::Taste), &HashMap::new(), None, &mut Rng::seeded(7)));
        assert!(picked.contains(&"k1".to_string()) || picked.contains(&"k2".to_string()), "{picked:?}");
    }

    #[test]
    fn a_skipped_track_comes_less_often_and_later_but_can_come() {
        let mut index = Index::new((0..10).map(|n| song(&format!("t{n}"), n, n, 1, 1990, &[])).collect());
        index.songs[0].skipped = 1.0;
        let (mut first, mut came, mut others) = (0, 0, 0);
        for n in 0..400u64 {
            let req = Ask { seed: vec![], exclude: vec![], last: None, count: Some(5), ..ask(Mode::Random) };
            let picked = ids(&index, &choose(&index, &req, &HashMap::new(), None, &mut Rng::seeded(n)));
            came += usize::from(picked.contains(&"t0".to_string()));
            others += usize::from(picked.contains(&"t1".to_string()));
            first += usize::from(picked[0] == "t0");
        }
        assert!(came > 0, "never left out");
        assert!(came * 3 < others, "less often: {came} against {others}");
        assert!(first * 10 < 400, "seldom first: {first}");
    }

    #[test]
    fn radio_inserts_keep_close_to_the_one_track_and_its_artist() {
        // The seed track's artist has three more songs on other albums, in its genre but other decades; strangers share
        // its genre and era. Artist (double) and genre beat genre and era.
        let mut s = vec![song("own", 1, 7, 1, 2000, &[]), song("a1", 2, 7, 1, 1980, &[]), song("a2", 3, 7, 1, 1975, &[]), song("a3", 4, 7, 1, 2020, &[])];
        s.extend((0..20).map(|n| song(&format!("g{n}"), 10 + n, 20 + n, 1, 2000, &[])));
        let index = Index::new(s);
        let req = Ask { mode: Mode::Radio, seed: vec!["own".into()], exclude: vec!["own".into()], last: Some("own".into()), count: Some(3), albums: false, by: RadioBy::Track, skip_favourites: false, similar_by: None, folders: None };
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
        let ask = |by| Ask {
            mode: Mode::Radio,
            seed: vec!["own".into()],
            exclude: vec!["own".into()],
            last: Some("own".into()),
            count: Some(3),
            albums: false,
            by,
            skip_favourites: false,
            similar_by: None,
            folders: None,
        };
        let era = ids(&index, &choose(&index, &ask(RadioBy::Era), &HashMap::new(), None, &mut Rng::seeded(4)));
        assert!(era.len() == 3 && era.iter().all(|p| p.starts_with("near")), "{era:?}");
        let artist = ids(&index, &choose(&index, &ask(RadioBy::Artist), &HashMap::new(), None, &mut Rng::seeded(4)));
        assert_eq!(artist.len(), 3);
        assert!(artist[..2].iter().all(|p| p.starts_with("own")), "the artist's two first, then Similar fills up: {artist:?}");
        let q: Ask = serde_json::from_str(r#"{"mode":"radio","by":"artist","count":10}"#).unwrap();
        assert_eq!((q.by, q.count), (RadioBy::Artist, Some(10)));
    }

    #[test]
    fn skipping_favourites_leaves_the_hearted_out() {
        let mut s: Vec<Song> = (0..8).map(|n| song(&format!("t{n}"), 1 + n, 10 + n, 1, 2000, &[])).collect();
        for x in &mut s[1..5] {
            x.favourite = true;
        }
        let index = Index::new(s);
        let ask = |skip_favourites| Ask {
            mode: Mode::Similar,
            seed: vec!["t0".into()],
            exclude: vec!["t0".into()],
            last: Some("t0".into()),
            count: Some(5),
            albums: false,
            by: RadioBy::Track,
            skip_favourites,
            similar_by: None,
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
        let index = Index::new(s).with_sounds(|id| vectors.get(id).cloned());
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
        let ask = Ask {
            mode: Mode::Similar,
            seed: vec!["t0".into()],
            exclude: vec!["t0".into()],
            last: Some("t0".into()),
            count: Some(5),
            albums: false,
            by: RadioBy::Track,
            skip_favourites: false,
            similar_by: None,
            folders: None,
        };
        let picked = ids(&index, &choose(&index, &ask, &HashMap::new(), None, &mut Rng::seeded(3)));
        assert!(picked.len() == 3 && picked.iter().all(|p| ["t5", "t6", "t7"].contains(&p.as_str())), "{picked:?}");
        // A whole album: its excluded track stays out.
        let mut a = vec![song("s1", 1, 1, 1, 2000, &[]), song("s2", 1, 1, 1, 2000, &[]), song("e1", 2, 2, 9, 1996, &[]), song("e2", 2, 2, 9, 1996, &[])];
        a[3].excluded = true;
        let index = Index::new(a);
        let whole = Ask { seed: vec!["s1".into(), "s2".into()], exclude: vec!["s1".into(), "s2".into()], last: Some("s2".into()), albums: true, ..ask };
        assert_eq!(ids(&index, &choose(&index, &whole, &HashMap::new(), None, &mut Rng::seeded(2))), ["e1"]);
    }

    /// Random's seed: s0 queued, s1 excluded, s2 a favourite, s3 in library 1, s4 and s5 plain. All in albums of their own,
    /// so the spacing decides nothing, and strangers in library 0 for what comes after the seed.
    fn seeded_at_random() -> (Index, Ask) {
        let mut s: Vec<Song> = (0..6).map(|n| song(&format!("s{n}"), 1 + n, 1 + n, 1, 2000, &[])).collect();
        s.extend((0..6).map(|n| song(&format!("x{n}"), 20 + n, 20 + n, 2, 2010, &[])));
        s[1].excluded = true;
        s[2].favourite = true;
        s[3].folders = vec![1];
        let req = Ask { mode: Mode::Random, seed: (0..6).map(|n| format!("s{n}")).collect(), exclude: vec!["s0".into()], last: None, count: Some(2), ..Ask::default() };
        (Index::new(s), req)
    }

    #[test]
    fn random_draws_from_the_seed_first_what_may_be_picked_of_it() {
        let (index, req) = seeded_at_random();
        let wide = Ask { count: Some(4), ..req.clone() };
        let seen: HashSet<String> = (0..40).flat_map(|n| ids(&index, &choose(&index, &wide, &HashMap::new(), None, &mut Rng::seeded(n)))).collect();
        assert_eq!(seen, HashSet::from(["s2", "s3", "s4", "s5"].map(String::from)), "every seed track left but the excluded one, and no stranger while they last");
        for (asked, gone) in [(Ask { skip_favourites: true, ..req.clone() }, "s2"), (Ask { folders: Some(vec![0]), ..req }, "s3")] {
            for n in 0..40 {
                let picked = ids(&index, &choose(&index, &asked, &HashMap::new(), None, &mut Rng::seeded(n)));
                assert!(picked.len() == 2 && !picked.iter().any(|p| p == gone || p == "s1"), "{gone}: {picked:?}");
            }
        }
    }

    #[test]
    fn random_inside_a_crate_draws_from_the_seed_only_the_crates_tracks() {
        let (index, req) = seeded_at_random();
        let crate_: Vec<usize> = ["s4", "x0", "x1"].iter().map(|id| index.by_id[*id]).collect();
        for n in 0..40 {
            let picked = ids(&index, &choose(&index, &req, &HashMap::new(), Some(&crate_), &mut Rng::seeded(n)));
            assert_eq!(picked[0], "s4", "the seed first");
            assert!(picked.iter().all(|p| ["s4", "x0", "x1"].contains(&p.as_str())), "only the crate's: {picked:?}");
        }
    }

    #[test]
    fn a_whole_album_holds_only_what_may_be_picked() {
        // Album 1 is the seed's, so album 2 is the one to come: e2 a favourite, e3 in library 1.
        let mut a = vec![song("s1", 1, 1, 1, 2000, &[]), song("s2", 1, 1, 1, 2000, &[]), song("e1", 2, 2, 9, 1996, &[]), song("e2", 2, 2, 9, 1996, &[]), song("e3", 2, 2, 9, 1996, &[])];
        a[3].favourite = true;
        a[4].folders = vec![1];
        let index = Index::new(a);
        let picked = |req: &Ask, within: Option<&[usize]>| ids(&index, &choose(&index, req, &HashMap::new(), within, &mut Rng::seeded(2)));
        let whole = Ask { albums: true, count: Some(5), ..ask(Mode::Similar) };
        assert_eq!(picked(&whole, None), ["e1", "e2", "e3"], "whole and in order, where nothing bars a track");
        assert_eq!(picked(&Ask { skip_favourites: true, ..whole.clone() }, None), ["e1", "e3"], "no favourite while they are ignored");
        assert_eq!(picked(&Ask { folders: Some(vec![0]), ..whole.clone() }, None), ["e1", "e2"], "nothing of a library not chosen");
        let crate_: Vec<usize> = ["e1", "e3"].iter().map(|id| index.by_id[*id]).collect();
        assert_eq!(picked(&whole, Some(&crate_)), ["e1", "e3"], "only the crate's tracks");
        assert_eq!(picked(&Ask { albums: false, ..ask(Mode::AlbumMix) }, Some(&crate_)), ["e1", "e3"], "Album mix is whole albums by its old name");
    }

    #[test]
    fn albums_give_one_whole_album_within_the_modes_limits() {
        // Seed album 1 (2000); album 2 (1995, the closest by era), album 3 (1960, out of Era's five years).
        let mut s = vec![song("s1", 1, 1, 1, 2000, &[]), song("s2", 1, 1, 1, 2000, &[])];
        s.extend([song("e1", 2, 2, 9, 1996, &[]), song("e2", 2, 2, 9, 1996, &[]), song("o1", 3, 1, 1, 1960, &[]), song("o2", 3, 1, 1, 1960, &[])]);
        let index = Index::new(s);
        let req = Ask {
            mode: Mode::Era,
            seed: vec!["s1".into(), "s2".into()],
            exclude: vec!["s1".into(), "s2".into()],
            last: Some("s2".into()),
            count: Some(5),
            albums: true,
            by: RadioBy::Track,
            skip_favourites: false,
            similar_by: None,
            folders: None,
        };
        let picked = ids(&index, &choose(&index, &req, &HashMap::new(), None, &mut Rng::seeded(2)));
        assert_eq!(picked, ["e1", "e2"], "the album in the era, whole, though album 3 shares the artist and genre");
    }
}
