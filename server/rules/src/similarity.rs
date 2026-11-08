//! One similarity (the owner, 2026-10-23: "One similarity is the foundation on which we will build"): what makes music
//! alike, worked out once for everything that asks: the radio, Autoplay and "more like this" (`app::radio`), and how
//! close the playlists are (`insights`). Pure: the caller hands in the tracks, their names already numbers.
//!
//! A set of tracks (one track, a playlist, a crate, the radio's seed) becomes a `Profile`. A profile is compared with a
//! track (`Profile::to_track`) or with another profile (`Profile::to_profile`), signal by signal, each 0 to 1 or unknown,
//! and one `Weights` table sums them.
//! - Words (playlists, artists, genres, album tags, AudioMuse's style tags) count by rarity (the owner, 2026-10-23):
//!   what many of the library's tracks have says little ("Electronic"), what few have says much ("Downtempo"). tf-idf
//!   over the whole library, compared by cosine.
//! - Numbers (year, BPM, AudioMuse's energy and danceability, CLAP's moods) keep their middle and their spread (the
//!   owner, 2026-10-23): a calm set and a calm-to-wild one are not alike for having the same middle. How far apart is
//!   nothing alike is the library's own: its interquartile range, what two of its tracks typically differ by.
//! - Sound: AudioMuse's "sounds like" vectors (decision 0054); a set's is the mean of its tracks'.
//! - Tracks: the tracks two sets share, over all they hold; between sets only.
//!
//! The weights are all equal until AudioMuse has heard the whole library; then they are weighed (docs/plans.md).

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// What two things can be alike in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    /// The tracks two sets share (between sets only).
    Tracks,
    /// The playlists the tracks are in: what people put together.
    Playlists,
    Artists,
    #[serde(rename = "genre")]
    Genres,
    /// The album tags.
    Tags,
    /// AudioMuse's style tags.
    #[serde(rename = "style")]
    Styles,
    /// AudioMuse's "sounds like".
    Sound,
    Era,
    Tempo,
    Energy,
    Danceability,
    /// CLAP's moods together.
    Mood,
}

const SIGNALS: usize = 12;

/// The kinds of words, in `Track::words`' order, and the signal each is.
const WORDS: [Signal; 5] = [Signal::Playlists, Signal::Artists, Signal::Genres, Signal::Tags, Signal::Styles];
/// CLAP's moods (its `other_features` but danceable), in `Track::moods`' order.
pub const MOODS: [&str; 5] = ["happy", "sad", "relaxed", "aggressive", "party"];
/// The numbers, in `Track::numbers`' order: year, BPM, energy, danceability, then the moods.
const NUMBERS: usize = 4 + MOODS.len();
const YEAR: usize = 0;
const NUMBER_SIGNALS: [Signal; 4] = [Signal::Era, Signal::Tempo, Signal::Energy, Signal::Danceability];

/// "Sounds like" on 0 to 1, from AudioMuse's cosine similarity: measured on the library's 8,909 vectors (2026-10-19),
/// two random tracks are 0.62 alike at the median: nothing alike. 1 is the same sound. (An album's tracks, 0.875 at
/// the median, were the top once; on the whole library a crowded sound's 1,000 closest are all above that, so it said
/// 100% of all of them: 2026-10-23.)
const SOUND_LO: f32 = 0.62;
const SOUND_HI: f32 = 1.0;
/// How far from 1 the length of a sound of unit length can read, for rounding alone: 4 steps at most in 40,000 vectors of
/// 200 numbers. Anything further is made so, as the shared cases' rounded sounds are: the nearest reads 20 steps away.
const UNIT_READS: f32 = 16.0 * f32::EPSILON;

/// One track as similarity sees it; its words as numbers, each kind its own.
#[derive(Debug, Clone, Default)]
pub struct Track {
    pub playlists: Vec<u32>,
    /// Its artists and its album artist.
    pub artists: Vec<u32>,
    pub genres: Vec<u32>,
    /// Its album's tags.
    pub tags: Vec<u32>,
    /// AudioMuse's style tags, its strongest.
    pub styles: Vec<u32>,
    pub year: Option<u32>,
    pub bpm: Option<f32>,
    /// AudioMuse's, 0 to 1.
    pub energy: Option<f32>,
    pub danceability: Option<f32>,
    /// CLAP's, in `MOODS`' order; none until CLAP heard it.
    pub moods: Option<[f32; MOODS.len()]>,
}

impl Track {
    fn words(&self) -> [&[u32]; WORDS.len()] {
        [&self.playlists, &self.artists, &self.genres, &self.tags, &self.styles]
    }

    fn numbers(&self) -> [Option<f32>; NUMBERS] {
        let mut out = [None; NUMBERS];
        out[..4].copy_from_slice(&[self.year.map(|y| y as f32), self.bpm, self.energy, self.danceability]);
        if let Some(m) = self.moods {
            for (o, x) in out[4..].iter_mut().zip(m) {
                *o = Some(x);
            }
        }
        out
    }
}

/// What the library is like as a whole: how rare each word is, how far apart its numbers typically are, and how each
/// track sounds. Built once with the tracks; `to_track` and `Profile::of` take tracks by their place in it.
#[derive(Debug, Clone, Default)]
pub struct Library {
    /// Per kind of word, each word's rarity: ln(tracks / tracks having it).
    idf: [Vec<f64>; WORDS.len()],
    /// Per number, what two tracks typically differ by: their interquartile range.
    reach: [f64; NUMBERS],
    /// Each track's "sounds like" vector, of unit length, where AudioMuse heard it. Shared with the facts it came from:
    /// the facts, the crates' library and the radio's each held a copy, 61 MB more of the NAS's 768.
    sounds: Vec<Option<Arc<[f32]>>>,
}

impl Library {
    /// `sounds`: by the tracks' places, as `tracks`; any length (missing ones unheard), any length of vector (unit
    /// length is made here, in a vector of its own; one of unit length already is shared as it is).
    pub fn new<'a>(tracks: impl IntoIterator<Item = &'a Track>, sounds: Vec<Option<Arc<[f32]>>>) -> Self {
        let mut df: [HashMap<u32, f64>; WORDS.len()] = Default::default();
        let mut values: [Vec<f64>; NUMBERS] = Default::default();
        let mut n = 0.0;
        for t in tracks {
            n += 1.0;
            for (k, words) in t.words().into_iter().enumerate() {
                for w in unique(words) {
                    *df[k].entry(w).or_default() += 1.0;
                }
            }
            for (k, x) in t.numbers().into_iter().enumerate() {
                values[k].extend(x.filter(|x| x.is_finite()).map(f64::from));
            }
        }
        let idf = df.map(|m| {
            let mut v = vec![0.0; m.keys().max().map_or(0, |&w| w as usize + 1)];
            for (w, d) in m {
                v[w as usize] = (n / d).ln();
            }
            v
        });
        let reach = values.map(|mut v| {
            v.sort_by(f64::total_cmp);
            let iqr = quantile(&v, 0.75) - quantile(&v, 0.25);
            let range = v.last().copied().unwrap_or(0.0) - v.first().copied().unwrap_or(0.0);
            // A library where most tracks agree: its whole range; one where all do: any difference is nothing alike.
            if iqr > 0.0 {
                iqr
            } else if range > 0.0 {
                range
            } else {
                1.0
            }
        });
        let sounds = sounds
            .into_iter()
            .map(|v| {
                let v = v?;
                let len = v.iter().map(|x| x * x).sum::<f32>().sqrt();
                if (len - 1.0).abs() <= UNIT_READS {
                    return Some(v);
                }
                (len > 0.0 && len.is_finite()).then(|| v.iter().map(|x| x / len).collect())
            })
            .collect();
        Library { idf, reach, sounds }
    }

    fn idf(&self, kind: usize, w: u32) -> f64 {
        self.idf[kind].get(w as usize).copied().unwrap_or(0.0)
    }

    pub fn sound(&self, i: usize) -> Option<&[f32]> {
        self.sounds.get(i)?.as_deref()
    }
}

/// A number of a set: its middle (median) and spread (half its interquartile range); a track's spread is 0.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Spread {
    middle: f64,
    spread: f64,
}

/// What a set of tracks is like.
#[derive(Debug, Clone, Default)]
pub struct Profile {
    /// How many tracks it holds, and how much they weigh together (as many, each weighing one).
    n: usize,
    mass: f64,
    /// Its tracks' places, in order.
    members: Vec<usize>,
    /// Per kind of word, how much of its tracks (by weight) have each word.
    counts: [BTreeMap<u32, f64>; WORDS.len()],
    /// Per kind of word, its tf-idf weights (the share of its tracks having the word times its rarity) and their length.
    /// In the words' order: floats summed in another order come out another in the last bits, and near ties fall the
    /// other way (one seed gives one answer).
    weights: [BTreeMap<u32, f64>; WORDS.len()],
    norms: [f64; WORDS.len()],
    numbers: [Option<Spread>; NUMBERS],
    /// The mean of its tracks' sounds, of unit length; none when none was heard.
    sound: Option<Vec<f32>>,
}

impl Profile {
    /// The profile of `set` (places in `lib`), `track` giving each place's track.
    pub fn of<'a>(lib: &Library, set: &[usize], track: impl Fn(usize) -> &'a Track) -> Self {
        let mut members = set.to_vec();
        members.sort_unstable();
        members.dedup();
        Self::of_weighted(lib, &members.into_iter().map(|i| (i, 1.0)).collect::<Vec<_>>(), track)
    }

    /// As `of`, each place counting as often as it is in `set` (a weighted seed: Sounds like your week's tracks by how
    /// often and how lately they were heard).
    pub fn of_counted<'a>(lib: &Library, set: &[usize], track: impl Fn(usize) -> &'a Track) -> Self {
        Self::of_weighted(lib, &set.iter().map(|&i| (i, 1.0)).collect::<Vec<_>>(), track)
    }

    /// As `of`, each place weighing what it is given (a taste: a favourite more than a track heard once); places
    /// weighing nothing are left out, a place given twice weighs both together.
    pub fn of_weighted<'a>(lib: &Library, set: &[(usize, f64)], track: impl Fn(usize) -> &'a Track) -> Self {
        let mut weight: BTreeMap<usize, f64> = BTreeMap::new();
        for &(i, w) in set.iter().filter(|(_, w)| w.is_finite() && *w > 0.0) {
            *weight.entry(i).or_default() += w;
        }
        let members: Vec<usize> = weight.keys().copied().collect();
        let mut p = Profile { n: members.len(), mass: weight.values().sum(), ..Profile::default() };
        let mut values: [Vec<(f64, f64)>; NUMBERS] = Default::default();
        for &i in &members {
            let (t, w) = (track(i), weight[&i]);
            for (k, words) in t.words().into_iter().enumerate() {
                for x in unique(words) {
                    *p.counts[k].entry(x).or_default() += w;
                }
            }
            for (k, x) in t.numbers().into_iter().enumerate() {
                values[k].extend(x.filter(|x| x.is_finite()).map(|x| (f64::from(x), w)));
            }
        }
        for (k, mut v) in values.into_iter().enumerate() {
            v.sort_by(|a, b| a.0.total_cmp(&b.0));
            if !v.is_empty() {
                let q = |at| weighted_quantile(&v, at);
                p.numbers[k] = Some(Spread { middle: q(0.5), spread: (q(0.75) - q(0.25)) / 2.0 });
            }
        }
        let heard: Vec<(&[f32], f32)> = members.iter().filter_map(|&i| Some((lib.sound(i)?, weight[&i] as f32))).collect();
        p.sound = mean_sound_weighted(&heard);
        p.members = members;
        p.weigh(lib);
        p
    }

    /// How many tracks it holds.
    pub fn tracks(&self) -> usize {
        self.n
    }

    /// Its tracks' places.
    pub fn members(&self) -> &[usize] {
        &self.members
    }

    /// Its words of a kind (`Signal::Playlists`, `Artists`, `Genres`, `Tags`, `Styles`), the most telling first: each
    /// with its weight, the share of its tracks having it times its rarity in the library.
    pub fn words(&self, kind: Signal) -> Vec<(u32, f64)> {
        let Some(k) = WORDS.iter().position(|w| *w == kind) else { return Vec::new() };
        let mut out: Vec<(u32, f64)> = self.weights[k].iter().map(|(&w, &x)| (w, x)).collect();
        out.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        out
    }

    /// A number's middle and spread (`Signal::Era`, `Tempo`, `Energy`, `Danceability`); none when none of its tracks
    /// has it.
    pub fn number(&self, s: Signal) -> Option<(f64, f64)> {
        let k = NUMBER_SIGNALS.iter().position(|x| *x == s)?;
        self.numbers[k].map(|n| (n.middle, n.spread))
    }

    /// CLAP's moods' middles and spreads, in `MOODS`' order; none where none of its tracks was heard.
    pub fn moods(&self) -> [Option<(f64, f64)>; MOODS.len()] {
        std::array::from_fn(|m| self.numbers[4 + m].map(|n| (n.middle, n.spread)))
    }

    /// The profile of a playlist: as `of`, without the playlist itself among its words (all its tracks are in it, so
    /// it would only say it is like itself).
    // The tests' own: nothing else uses it (no warning in the server's build).
    #[cfg(test)]
    pub fn of_playlist<'a>(lib: &Library, playlist: u32, set: &[usize], track: impl Fn(usize) -> &'a Track) -> Self {
        let mut p = Self::of(lib, set, track);
        p.counts[0].remove(&playlist);
        p.weigh(lib);
        p
    }

    fn weigh(&mut self, lib: &Library) {
        let n = if self.mass > 0.0 { self.mass } else { 1.0 };
        for k in 0..WORDS.len() {
            self.weights[k] =
                self.counts[k].iter().map(|(&w, &c)| (w, c as f64 / n * lib.idf(k, w))).filter(|(_, x)| *x > 0.0).collect();
            self.norms[k] = self.weights[k].values().map(|x| x * x).sum::<f64>().sqrt();
        }
    }

    /// Its middle year.
    pub fn year(&self) -> Option<u32> {
        self.numbers[YEAR].map(|s| s.middle.round() as u32)
    }

    /// How much of its tracks (by weight) have each artist.
    pub fn artists(&self) -> &BTreeMap<u32, f64> {
        &self.counts[1]
    }

    /// How alike a track (at place `i` in `lib`) is to this set.
    pub fn to_track(&self, lib: &Library, i: usize, t: &Track) -> Signals {
        let mut out = Signals::default();
        if self.n == 0 {
            return out;
        }
        for (k, words) in t.words().into_iter().enumerate() {
            if self.norms[k] == 0.0 {
                continue;
            }
            let (mut dot, mut len) = (0.0, 0.0);
            for w in unique(words) {
                let x = lib.idf(k, w);
                len += x * x;
                dot += x * self.weights[k].get(&w).copied().unwrap_or(0.0);
            }
            out.set(WORDS[k], if len == 0.0 { 0.0 } else { (dot / (len.sqrt() * self.norms[k])).clamp(0.0, 1.0) });
        }
        let theirs = t.numbers();
        self.numbers_against(lib, |k| theirs[k].filter(|x| x.is_finite()).map(|x| Spread { middle: x as f64, spread: 0.0 }), true, &mut out);
        if let (Some(p), Some(v)) = (&self.sound, lib.sound(i)) {
            out.set(Signal::Sound, sounds_alike(p, v));
        }
        out
    }

    /// How alike two sets are.
    pub fn to_profile(&self, lib: &Library, other: &Profile) -> Signals {
        let mut out = Signals::default();
        if self.n == 0 || other.n == 0 {
            return out;
        }
        let shared = sorted_shared(&self.members, &other.members);
        out.set(Signal::Tracks, shared as f64 / (self.n + other.n - shared) as f64);
        for k in 0..WORDS.len() {
            if self.norms[k] == 0.0 || other.norms[k] == 0.0 {
                continue;
            }
            let dot: f64 = self.weights[k].iter().filter_map(|(w, x)| other.weights[k].get(w).map(|y| x * y)).sum();
            out.set(WORDS[k], (dot / (self.norms[k] * other.norms[k])).clamp(0.0, 1.0));
        }
        self.numbers_against(lib, |k| other.numbers[k], false, &mut out);
        if let (Some(a), Some(b)) = (&self.sound, &other.sound) {
            out.set(Signal::Sound, sounds_alike(a, b));
        }
        out
    }

    /// The number signals against another's numbers. A track (`track`) is close anywhere within this set's spread;
    /// a set is close with the same middle and the same spread.
    fn numbers_against(&self, lib: &Library, theirs: impl Fn(usize) -> Option<Spread>, track: bool, out: &mut Signals) {
        let mut close = [None; NUMBERS];
        for k in 0..NUMBERS {
            let (Some(a), Some(b)) = (self.numbers[k], theirs(k)) else { continue };
            let apart = if track { ((a.middle - b.middle).abs() - a.spread).max(0.0) } else { (a.middle - b.middle).abs() + (a.spread - b.spread).abs() };
            close[k] = Some((1.0 - apart / lib.reach[k]).clamp(0.0, 1.0));
        }
        for (k, s) in NUMBER_SIGNALS.into_iter().enumerate() {
            if let Some(x) = close[k] {
                out.set(s, x);
            }
        }
        // Their mean, summed as a list of them would be: nothing made for it, as this runs for every track.
        let moods = close[4..].iter().flatten();
        let known = moods.clone().count();
        if known > 0 {
            out.set(Signal::Mood, moods.sum::<f64>() / known as f64);
        }
    }
}

/// Several profiles against the library in one pass (the tablet, 2026-11-07: a Sounds like of six sounds was six passes, 708 ms
/// against a budget of 300): what is the track's alone is worked out once, however many ask, and each profile's weights are read by
/// the word's number, not looked for in a map. Nothing is made anew for a track, and the profiles' sounds lie side by side, so one
/// walk down a track's sound reads them all at once. Each sum still adds the same products in the same order, so each answer is
/// `Profile::to_track`'s to the bit.
pub struct Together<'a> {
    lib: &'a Library,
    profiles: &'a [Profile],
    /// Per profile, per kind of word, each word's weight by its number.
    weights: Vec<[Vec<f64>; WORDS.len()]>,
    /// The track's words of each kind, each with its rarity: kept between tracks, so none is made anew.
    words: [Vec<(u32, f64)>; WORDS.len()],
    /// The profiles whose sound is `dims` long, a lane each, and those sounds side by side, `LANES` lanes a block and number by
    /// number within it (`[block * dims + j][lane]`); a lane no profile takes is 0.
    lanes: Vec<usize>,
    side: Vec<[f32; LANES]>,
    dims: usize,
    /// Each profile's sound against the track's, by its lane.
    sums: Vec<f32>,
}

/// How many profiles' sounds one walk down a track's sound reads together: as many sums as a processor adds at once, and no more
/// than a varied seed's groups (`crates`' six) leave idle.
const LANES: usize = 8;

impl<'a> Together<'a> {
    pub fn of(lib: &'a Library, profiles: &'a [Profile]) -> Self {
        let dense = |p: &Profile| -> [Vec<f64>; WORDS.len()] {
            std::array::from_fn(|k| {
                let mut by_word = vec![0.0; p.weights[k].keys().next_back().map_or(0, |&w| w as usize + 1)];
                for (&w, &x) in &p.weights[k] {
                    by_word[w as usize] = x;
                }
                by_word
            })
        };
        let dims = profiles.iter().find_map(|p| p.sound.as_ref()).map_or(0, Vec::len);
        let lanes: Vec<usize> = (0..profiles.len()).filter(|&at| profiles[at].sound.as_ref().is_some_and(|s| s.len() == dims)).collect();
        let mut side = vec![[0.0f32; LANES]; lanes.len().div_ceil(LANES) * dims];
        for (lane, &at) in lanes.iter().enumerate() {
            let sound = profiles[at].sound.as_deref().unwrap_or_default();
            for (j, &x) in sound.iter().enumerate() {
                side[lane / LANES * dims + j][lane % LANES] = x;
            }
        }
        let sums = vec![0.0; lanes.len()];
        Together { lib, profiles, weights: profiles.iter().map(dense).collect(), words: Default::default(), lanes, side, dims, sums }
    }

    /// Every lane's sound against `v` (`dims` long), as `sounds_alike` sums one: from -0.0, the products in the sound's order.
    fn sound_sums(&mut self, v: &[f32]) {
        for (block, sums) in self.side.chunks_exact(self.dims.max(1)).zip(self.sums.chunks_mut(LANES)) {
            let mut acc = [-0.0f32; LANES];
            for (row, &y) in block.iter().zip(v) {
                for lane in 0..LANES {
                    acc[lane] += row[lane] * y;
                }
            }
            sums.copy_from_slice(&acc[..sums.len()]);
        }
    }

    /// How alike the track at place `i` is to each profile, in their order: `each(which, signals)`.
    pub fn to_track(&mut self, i: usize, t: &Track, mut each: impl FnMut(usize, Signals)) {
        let lib = self.lib;
        let kinds = t.words();
        for (k, list) in self.words.iter_mut().enumerate() {
            list.clear();
            list.extend(unique(kinds[k]).map(|w| (w, lib.idf(k, w))));
        }
        let lengths = self.words.each_ref().map(|list| list.iter().fold(0.0, |len, (_, x)| len + x * x));
        // The track's numbers as a set of one, once for all the profiles.
        let theirs = t.numbers().map(|x| x.filter(|x| x.is_finite()).map(|x| Spread { middle: x as f64, spread: 0.0 }));
        let sound = lib.sound(i);
        let summed = sound.filter(|v| v.len() == self.dims && !self.lanes.is_empty());
        if let Some(v) = summed {
            self.sound_sums(v);
        }
        let mut lane = 0;
        for (at, p) in self.profiles.iter().enumerate() {
            let mut out = Signals::default();
            let in_lane = self.lanes.get(lane) == Some(&at);
            if in_lane {
                lane += 1;
            }
            if p.n == 0 {
                each(at, out);
                continue;
            }
            for k in 0..WORDS.len() {
                if p.norms[k] == 0.0 {
                    continue;
                }
                let weights = &self.weights[at][k];
                let dot = self.words[k].iter().fold(0.0, |dot, (w, x)| dot + x * weights.get(*w as usize).copied().unwrap_or(0.0));
                let len = lengths[k];
                out.set(WORDS[k], if len == 0.0 { 0.0 } else { (dot / (len.sqrt() * p.norms[k])).clamp(0.0, 1.0) });
            }
            p.numbers_against(lib, |k| theirs[k], true, &mut out);
            if let (Some(s), Some(v)) = (&p.sound, sound) {
                out.set(Signal::Sound, if in_lane && summed.is_some() { sound_alike(self.sums[lane - 1]) } else { sounds_alike(s, v) });
            }
            each(at, out);
        }
    }
}

/// How alike two things are on each signal, 0 to 1; none where it is not known (a track AudioMuse has not heard, a
/// set without genres).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Signals([Option<f64>; SIGNALS]);

impl Signals {
    fn set(&mut self, s: Signal, x: f64) {
        self.0[s as usize] = Some(x);
    }

    // The tests' own: nothing else uses it (no warning in the server's build).
    #[cfg(test)]
    pub fn get(&self, s: Signal) -> Option<f64> {
        self.0[s as usize]
    }

    /// All of them, by the weights: the weighed mean of the signals known, 0 when none is.
    /// Whether anything at all is known of how alike the two are.
    pub fn known(&self) -> bool {
        self.0.iter().any(Option::is_some)
    }

    pub fn score(&self, w: &Weights) -> f64 {
        let (mut sum, mut of) = (0.0, 0.0);
        for (x, w) in self.0.iter().zip(w.0) {
            if let Some(x) = x {
                sum += w * x;
                of += w;
            }
        }
        if of == 0.0 { 0.0 } else { sum / of }
    }
}

/// How much each signal counts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weights([f64; SIGNALS]);

impl Weights {
    /// Every signal the same (the owner, 2026-10-16, for the radio's five; all, until they are weighed).
    pub const EQUAL: Weights = Weights([1.0; SIGNALS]);

    pub const fn with(mut self, s: Signal, w: f64) -> Self {
        self.0[s as usize] = w;
        self
    }

    /// The listener's own order ("Similar by", the owner 2026-10-23): only the signals named, the first counting
    /// most, the next a half, a third, and so on, so all of them still count. None named: all equal.
    pub fn ranked(order: &[Signal]) -> Self {
        if order.is_empty() {
            return Weights::EQUAL;
        }
        let mut w = Weights([0.0; SIGNALS]);
        for (k, s) in order.iter().enumerate() {
            if w.0[*s as usize] == 0.0 {
                w.0[*s as usize] = 1.0 / (k + 1) as f64;
            }
        }
        w
    }
}

/// How several sounds sound together: their mean, of unit length; none when none was heard.
pub fn mean_sound(heard: &[&[f32]]) -> Option<Vec<f32>> {
    mean_sound_weighted(&heard.iter().map(|v| (*v, 1.0)).collect::<Vec<_>>())
}

/// As `mean_sound`, each sound counting by its weight (Sounds like your week: how often and how lately it was heard).
pub fn mean_sound_weighted(heard: &[(&[f32], f32)]) -> Option<Vec<f32>> {
    let dims = heard.first()?.0.len();
    let mut mean = vec![0.0f32; dims];
    for (v, w) in heard.iter().filter(|(v, w)| v.len() == dims && *w > 0.0) {
        for (m, x) in mean.iter_mut().zip(v.iter()) {
            *m += w * x;
        }
    }
    let len = mean.iter().map(|x| x * x).sum::<f32>().sqrt();
    (len > 0.0 && len.is_finite()).then(|| mean.into_iter().map(|x| x / len).collect())
}

/// "Sounds like" on 0 to 1, of two sounds of unit length.
pub fn sounds_alike(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() {
        return 0.0;
    }
    sound_alike(a.iter().zip(b).map(|(x, y)| x * y).sum())
}

/// "Sounds like" on 0 to 1, of the cosine of two sounds.
pub fn sound_alike(cos: f32) -> f64 {
    ((cos - SOUND_LO) / (SOUND_HI - SOUND_LO)).clamp(0.0, 1.0) as f64
}

/// How many places two sorted lists share.
fn sorted_shared(a: &[usize], b: &[usize]) -> usize {
    let (mut i, mut j, mut n) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                n += 1;
                i += 1;
                j += 1;
            }
        }
    }
    n
}

/// The value at `q` (0..1) of sorted values, between neighbours; 0 for none.
/// As `quantile`, each value weighing its weight (a value of weight 2 as two of it), sorted by value.
fn weighted_quantile(sorted: &[(f64, f64)], q: f64) -> f64 {
    let total: f64 = sorted.iter().map(|v| v.1).sum();
    if sorted.is_empty() || total <= 0.0 {
        return 0.0;
    }
    // The value at a position counted in weight, from 0: the one whose weight covers it.
    let at_pos = |pos: f64| {
        let mut before = 0.0;
        for &(x, w) in sorted {
            if pos < before + w {
                return x;
            }
            before += w;
        }
        sorted[sorted.len() - 1].0
    };
    let at = (q * (total - 1.0)).max(0.0);
    let (lo, hi) = (at.floor(), at.ceil());
    let (a, b) = (at_pos(lo), at_pos(hi));
    a + (b - a) * (at - lo)
}

pub fn quantile(sorted: &[f64], q: f64) -> f64 {
    match sorted.len() {
        0 => 0.0,
        n => {
            let at = q * (n - 1) as f64;
            let (lo, hi) = (at.floor() as usize, at.ceil() as usize);
            sorted[lo] + (sorted[hi] - sorted[lo]) * (at - lo as f64)
        }
    }
}

pub fn unique(items: &[u32]) -> impl Iterator<Item = u32> + '_ {
    items.iter().enumerate().filter(move |(i, x)| !items[..*i].contains(x)).map(|(_, x)| *x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(genre: u32, year: u32, energy: f32) -> Track {
        Track { genres: vec![genre], year: Some(year), energy: Some(energy), ..Track::default() }
    }

    #[test]
    fn a_rare_shared_word_counts_more_than_a_common_one() {
        // Genre 1 on every track but one; genre 2 on two of them.
        let mut tracks: Vec<Track> = (0..20).map(|_| Track { genres: vec![1], ..Track::default() }).collect();
        tracks[0].genres = vec![1, 2];
        tracks[1].genres = vec![1, 2];
        tracks[19].genres = vec![3];
        let lib = Library::new(&tracks, Vec::new());
        let seed = Profile::of(&lib, &[0], |i| &tracks[i]);
        let rare = seed.to_track(&lib, 1, &tracks[1]).get(Signal::Genres).unwrap();
        let common = seed.to_track(&lib, 5, &tracks[5]).get(Signal::Genres).unwrap();
        let none = seed.to_track(&lib, 19, &tracks[19]).get(Signal::Genres).unwrap();
        assert!(rare > 0.99 && common < 0.1 && none == 0.0, "{rare} {common} {none}");
    }

    #[test]
    fn sets_with_one_middle_but_another_spread_are_told_apart() {
        // Energy from 0 to 1 over the library, so its interquartile range is 0.5.
        let tracks: Vec<Track> = (0..=10).map(|n| track(1, 2000, n as f32 / 10.0)).collect();
        let lib = Library::new(&tracks, Vec::new());
        let calm = Profile::of(&lib, &[4, 5, 6], |i| &tracks[i]);
        let calm_too = Profile::of(&lib, &[4, 5, 5, 6], |i| &tracks[i]);
        let wide = Profile::of(&lib, &[0, 1, 5, 9, 10], |i| &tracks[i]);
        let energy = |a: &Profile, b: &Profile| a.to_profile(&lib, b).get(Signal::Energy).unwrap();
        assert!(energy(&calm, &calm_too) > 0.99);
        assert!(energy(&calm, &wide) < 0.5, "{}", energy(&calm, &wide));
        // A track inside the wide set's spread is as close to it as one at its middle.
        assert_eq!(wide.to_track(&lib, 3, &tracks[3]).get(Signal::Energy), wide.to_track(&lib, 5, &tracks[5]).get(Signal::Energy));
    }

    #[test]
    fn what_is_not_known_does_not_count() {
        let tracks = vec![track(1, 1990, 0.5), Track { genres: vec![1], year: Some(1990), ..Track::default() }, track(2, 2020, 0.9)];
        let lib = Library::new(&tracks, vec![Some(Arc::from([1.0, 0.0])), None, Some(Arc::from([0.0, 1.0]))]);
        let seed = Profile::of(&lib, &[0], |i| &tracks[i]);
        let unheard = seed.to_track(&lib, 1, &tracks[1]);
        assert_eq!((unheard.get(Signal::Energy), unheard.get(Signal::Sound), unheard.get(Signal::Tracks)), (None, None, None));
        assert_eq!(unheard.score(&Weights::EQUAL), 1.0, "alike in all that is known");
        assert_eq!(seed.to_track(&lib, 2, &tracks[2]).score(&Weights::EQUAL), 0.0);
    }

    /// Numbers as a track's sound holds them, the same every time.
    fn heard(seed: u64, dims: usize) -> Vec<f32> {
        let mut r = crate::Rng::seeded(seed);
        (0..dims).map(|_| r.unit() as f32 - 0.5).collect()
    }

    #[test]
    fn a_sound_of_unit_length_is_shared_and_any_other_is_made_so_in_a_vector_of_its_own() {
        let tracks = vec![Track::default(); 3];
        let (unit, long): (Arc<[f32]>, Arc<[f32]>) = (Arc::from(mean_sound(&[&heard(1, 200)]).unwrap()), Arc::from(heard(2, 200)));
        let lib = Library::new(&tracks, vec![Some(unit.clone()), Some(long.clone()), Some(Arc::from([0.0, 0.0]))]);
        assert!(Arc::ptr_eq(lib.sounds[0].as_ref().unwrap(), &unit));
        let made = lib.sounds[1].as_ref().unwrap();
        let len = made.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(!Arc::ptr_eq(made, &long) && (len - 1.0).abs() <= UNIT_READS, "{len}");
        assert_eq!(lib.sound(2), None, "all zeros: no length to make so");
    }

    #[test]
    fn rounding_never_turns_a_shared_sound_into_a_copy() {
        for dims in [16, 200, 512] {
            let sounds: Vec<Arc<[f32]>> = (0..4000).map(|seed| Arc::from(mean_sound(&[&mean_sound(&[&heard(seed, dims)]).unwrap()]).unwrap())).collect();
            let tracks = vec![Track::default(); sounds.len()];
            let lib = Library::new(&tracks, sounds.iter().cloned().map(Some).collect());
            assert!(sounds.iter().zip(&lib.sounds).all(|(s, held)| held.as_ref().is_some_and(|h| Arc::ptr_eq(h, s))), "{dims} numbers");
        }
    }

    #[test]
    fn an_order_counts_the_first_most_and_leaves_out_the_rest() {
        let mut s = Signals::default();
        s.set(Signal::Sound, 1.0);
        s.set(Signal::Era, 0.0);
        s.set(Signal::Genres, 0.0);
        assert!((s.score(&Weights::ranked(&[Signal::Sound, Signal::Era])) - 2.0 / 3.0).abs() < 1e-9);
        assert!((s.score(&Weights::ranked(&[Signal::Era, Signal::Sound])) - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(s.score(&Weights::ranked(&[Signal::Sound])), 1.0, "genre and era off");
        assert_eq!(s.score(&Weights::ranked(&[Signal::Mood])), 0.0, "nothing known of what counts");
    }

    /// Many words of many rarities, so the order of their sums shows in the last bits; a map's order differs from map to
    /// map, and a near tie must not fall one way here and the other there.
    #[test]
    fn a_profile_comes_out_the_same_to_the_last_bit_every_time() {
        let tracks: Vec<Track> = (0..300u32).map(|n| Track { genres: (0..=n % 40).map(|g| (g * 7 + n % 3) % 97).collect(), ..Track::default() }).collect();
        let lib = Library::new(&tracks, Vec::new());
        let set: Vec<(usize, f64)> = (0..300).map(|i| (i, 1.0 + i as f64 / 7.0)).collect();
        let other = Profile::of(&lib, &[1, 22, 123, 244], |i| &tracks[i]);
        let once = || {
            let p = Profile::of_weighted(&lib, &set, |i| &tracks[i]);
            (p.mass.to_bits(), p.norms.map(f64::to_bits), p.to_profile(&lib, &other).get(Signal::Genres).map(f64::to_bits))
        };
        let first = once();
        for _ in 0..50 {
            assert_eq!(once(), first);
        }
    }

    /// One pass over the library for several profiles is that many passes, to the last bit of every signal: words of many rarities and
    /// some known to no profile, numbers, moods and sounds, and tracks missing some of them.
    #[test]
    fn several_profiles_in_one_pass_read_to_the_bit_as_each_alone() {
        let tracks: Vec<Track> = (0..200u32)
            .map(|n| Track {
                genres: (0..n % 9).map(|g| (g * 7 + n % 3) % 41).collect(),
                artists: vec![n % 13, n % 13],
                tags: if n % 4 == 0 { Vec::new() } else { vec![n % 5, 1000 + n % 2] },
                year: (n % 3 != 0).then_some(1970 + n % 50),
                bpm: (n % 2 == 0).then_some(80.0 + (n % 90) as f32),
                energy: Some((n % 10) as f32 / 10.0),
                moods: (n % 5 != 0).then_some([0.1, 0.2, (n % 7) as f32 / 7.0, 0.4, 0.5]),
                ..Track::default()
            })
            .collect();
        let sounds: Vec<Option<Arc<[f32]>>> = (0..200u64).map(|n| (n % 6 != 0).then(|| heard(n, 16).into())).collect();
        let lib = Library::new(&tracks, sounds);
        let sets: [Vec<usize>; 4] = [vec![1, 2, 3], (10..40).collect(), vec![0], Vec::new()];
        let profiles: Vec<Profile> = sets.iter().map(|set| Profile::of_counted(&lib, set, |i| &tracks[i])).collect();
        let mut together = Together::of(&lib, &profiles);
        for (i, t) in tracks.iter().enumerate() {
            let mut told = 0;
            together.to_track(i, t, |at, signals| {
                let alone = profiles[at].to_track(&lib, i, t);
                assert_eq!(signals.0.map(|s| s.map(f64::to_bits)), alone.0.map(|s| s.map(f64::to_bits)), "track {i}, profile {at}");
                told += 1;
            });
            assert_eq!(told, profiles.len(), "every profile hears of every track");
        }
    }

    /// Past a block of lanes, and with sounds of two lengths in the library (a profile's sound is as long as its first heard track's),
    /// one pass still reads to the bit as each profile alone.
    #[test]
    fn more_profiles_than_a_block_and_sounds_of_two_lengths_read_to_the_bit_as_each_alone() {
        let tracks: Vec<Track> = (0..118u32).map(|n| Track { genres: vec![n % 7], energy: Some((n % 11) as f32 / 10.0), ..Track::default() }).collect();
        // Sounds near one another, so a sum read from another track's would not come out as nothing alike.
        let near = |n: u64, dims: usize| -> Arc<[f32]> { heard(n, dims).iter().map(|x| 1.0 + x * 0.2).collect() };
        let sounds: Vec<Option<Arc<[f32]>>> = (0..118u64).map(|n| (n % 9 != 0).then(|| near(n, if n % 10 == 3 { 8 } else { 24 }))).collect();
        let lib = Library::new(&tracks, sounds);
        let sets: Vec<Vec<usize>> = (0..LANES + 3).map(|g| (g * 7..g * 7 + 5).collect()).chain([vec![3], vec![13, 23], Vec::new()]).collect();
        let profiles: Vec<Profile> = sets.iter().map(|set| Profile::of_counted(&lib, set, |i| &tracks[i])).collect();
        let short = profiles.iter().filter(|p| p.sound.as_ref().is_some_and(|s| s.len() == 8)).count();
        assert!(short > 0 && profiles.iter().filter(|p| p.sound.is_some()).count() - short > LANES, "both lengths, and past a block");
        let mut together = Together::of(&lib, &profiles);
        for (i, t) in tracks.iter().enumerate() {
            together.to_track(i, t, |at, signals| {
                let alone = profiles[at].to_track(&lib, i, t);
                assert_eq!(signals.0.map(|s| s.map(f64::to_bits)), alone.0.map(|s| s.map(f64::to_bits)), "track {i}, profile {at}");
            });
        }
    }

    /// CLAP's moods are one signal, the mean of how close each is.
    #[test]
    fn the_moods_are_one_signal_the_mean_of_each() {
        let tracks = vec![Track { moods: Some([0.0; 5]), ..Track::default() }, Track { moods: Some([1.0, 0.0, 0.0, 0.0, 0.0]), ..Track::default() }];
        let lib = Library::new(&tracks, Vec::new());
        let seed = Profile::of(&lib, &[0], |i| &tracks[i]);
        assert_eq!(seed.to_track(&lib, 1, &tracks[1]).get(Signal::Mood), Some(4.0 / 5.0), "one far apart, four the same");
    }

    #[test]
    fn a_playlist_is_not_alike_to_another_for_being_itself() {
        let tracks: Vec<Track> = (0..6).map(|n| Track { playlists: if n < 3 { vec![1] } else { vec![2] }, ..Track::default() }).collect();
        let lib = Library::new(&tracks, Vec::new());
        let a = Profile::of_playlist(&lib, 1, &[0, 1, 2], |i| &tracks[i]);
        let b = Profile::of_playlist(&lib, 2, &[3, 4, 5], |i| &tracks[i]);
        assert_eq!(a.to_profile(&lib, &b).get(Signal::Playlists), None, "nothing else to be in");
        assert_eq!(a.to_profile(&lib, &b).get(Signal::Tracks), Some(0.0));
        assert_eq!(a.to_profile(&lib, &a).get(Signal::Tracks), Some(1.0));
    }
}
