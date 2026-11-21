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
//!
//! What the seed knows decides (SIM, the owner, 2026-11-21): a signal the seed has and the other lacks counts as not alike (0),
//! never skipped; one the seed lacks is left out. What is shown (similar albums, playlists, crates and artists, Radio's inserts)
//! holds a floor, `ALIKE_FLOOR`, and must be alike on something telling (`Signals::shown`); a seed known by too little shows
//! nothing (`Profile::too_little`).

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

/// The least a candidate is alike to be shown (SIM rule 4), on the 0 to 1 score of the surface that shows it, read before any
/// rounding, jitter or nudge: 0.30, provisional until SIM.19's count on a copy of the library fixes it.
pub const ALIKE_FLOOR: f64 = 0.30;

/// A library at least this big has words too common to tell anything: one on more than a quarter of its tracks (SIM, T-SIM-6). A
/// smaller one is never emptied by its own common words.
const COMMON_FROM: f64 = 1000.0;

/// Whether a word names a decade ("80s"): a number in words, never telling (SIM, T-SIM-6), whatever the library's size.
pub fn era_word(word: &str) -> bool {
    matches!(word.trim().to_lowercase().as_str(), "60s" | "70s" | "80s" | "90s" | "00s")
}

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
    /// Per kind of word, whether each word tells anything when two share it (SIM rule 6, T-SIM-6): not on every track, not on
    /// more than a quarter of a library of `COMMON_FROM` tracks or more, and no decade (`untold`).
    told: [Vec<bool>; WORDS.len()],
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
        let told = df.each_ref().map(|m| {
            let mut v = vec![false; m.keys().max().map_or(0, |&w| w as usize + 1)];
            for (&w, &d) in m {
                v[w as usize] = d < n && !(n >= COMMON_FROM && 4.0 * d > n);
            }
            v
        });
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
        Library { idf, reach, sounds, told }
    }

    /// With these words of a kind telling nothing when shared: the decades (`era_word`), which only the caller can read, as here
    /// a word is a number.
    pub fn untold(mut self, kind: Signal, words: impl IntoIterator<Item = u32>) -> Self {
        if let Some(k) = WORDS.iter().position(|w| *w == kind) {
            for w in words {
                if let Some(t) = self.told[k].get_mut(w as usize) {
                    *t = false;
                }
            }
        }
        self
    }

    /// Whether two sharing this word of a kind (its index in `WORDS`) are alike on something telling (SIM rule 6).
    fn telling(&self, kind: usize, w: u32) -> bool {
        self.told[kind].get(w as usize).copied().unwrap_or(false)
    }

    /// What two of its tracks typically differ by in a number (`Signal::Era`, `Tempo`, `Energy`, `Danceability`): how far
    /// apart is nothing alike.
    pub fn reach(&self, s: Signal) -> Option<f64> {
        NUMBER_SIGNALS.iter().position(|x| *x == s).map(|k| self.reach[k])
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
    /// Per kind of word, whether it holds a telling word (`Library::telling`): only so is it known by that kind (SIM rule 3).
    telling: [bool; WORDS.len()],
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
    /// it would only say it is like itself; SIM, Q-SIM-6: its own membership is no kind it is known by).
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
            self.telling[k] = self.weights[k].keys().any(|&w| lib.telling(k, w));
        }
    }

    /// How many kinds of signal it is known by (SIM rule 3, T-SIM-2), of those the weights count (a kind counts where one of its
    /// signals weighs above 0): a word kind through a telling word, the others through any of their signals.
    pub fn kinds(&self, w: &Weights) -> usize {
        // SIM's red commit: none yet.
        let _ = (w, self.telling);
        0
    }

    /// Known by too little to show anything alike (SIM rule 3, T-SIM-7): fewer than two of the kinds the weights count, or,
    /// where they count one kind alone, not that one.
    pub fn too_little(&self, w: &Weights) -> bool {
        // SIM's red commit: never yet.
        let _ = w;
        false
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

    /// The score by the weights where it may be shown (SIM rules 4 and 6): at or above `ALIKE_FLOOR`, and alike on something
    /// telling (a shared track, or a shared telling word of any kind, or a sound more alike than chance), whatever the weights
    /// count; else none. Numbers alone never make a shown match.
    pub fn shown(&self, w: &Weights) -> Option<f64> {
        // SIM's red commit: everything, as today.
        Some(self.score(w))
    }

    /// Whether the other had a signal the seed has: compared on both sides.
    // The tests' own: nothing else uses it (no warning in the server's build).
    #[cfg(test)]
    pub fn had(&self, s: Signal) -> bool {
        self.0[s as usize].is_some()
    }

    /// As alike on `s` as its value says, and that telling (the floor's own cases).
    #[cfg(test)]
    fn alike_on(mut self, s: Signal, x: f64) -> Self {
        self.set(s, x);
        self
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

    /// These weights on `keep` alone, every other signal counting nothing (Bandcamp's suggestions, compared like for like on
    /// what a suggestion can be known by: SIM, Q-SIM-5).
    pub fn only(mut self, keep: &[Signal]) -> Self {
        for (s, w) in self.0.iter_mut().enumerate() {
            if !keep.iter().any(|k| *k as usize == s) {
                *w = 0.0;
            }
        }
        self
    }

    /// Whether a signal counts at all.
    pub fn counts(&self, s: Signal) -> bool {
        self.0[s as usize] > 0.0
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

    /// SIM (change by design, from `what_is_not_known_does_not_count`): what the seed was heard by and the track was not counts as
    /// not alike; a track is never set apart by the tracks it shares (between sets only).
    #[test]
    fn what_the_seed_knows_and_a_track_lacks_counts_as_not_alike() {
        let tracks = vec![track(1, 1990, 0.5), Track { genres: vec![1], year: Some(1990), ..Track::default() }, track(2, 2020, 0.9)];
        let lib = Library::new(&tracks, vec![Some(Arc::from([1.0, 0.0])), None, Some(Arc::from([0.0, 1.0]))]);
        let seed = Profile::of(&lib, &[0], |i| &tracks[i]);
        let unheard = seed.to_track(&lib, 1, &tracks[1]);
        assert_eq!((unheard.get(Signal::Energy), unheard.get(Signal::Sound), unheard.get(Signal::Tracks)), (Some(0.0), Some(0.0), None));
        assert_eq!(unheard.score(&Weights::EQUAL), 0.5, "genre and era alike, energy and sound not: (1 + 1 + 0 + 0) / 4");
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

    // --- SIM (SCENARIOS-SIM.md): what the seed knows decides; too little known; the floor; something telling ---

    use Signal::{Artists, Danceability, Energy, Era, Genres, Mood, Playlists, Sound, Styles, Tags, Tempo, Tracks};

    /// Every signal a track can have, in `Signal`'s order: what a twin lacks one at a time.
    const HAS: [Signal; 11] = [Playlists, Artists, Genres, Tags, Styles, Sound, Era, Tempo, Energy, Danceability, Mood];

    /// A track as the scenarios write it: its words by name.
    #[derive(Clone, Default)]
    struct Said {
        playlists: Vec<&'static str>,
        artists: Vec<&'static str>,
        genres: Vec<&'static str>,
        tags: Vec<&'static str>,
        styles: Vec<&'static str>,
        year: Option<u32>,
        bpm: Option<f32>,
        energy: Option<f32>,
        danceability: Option<f32>,
        moods: Option<[f32; 5]>,
        sound: Option<Vec<f32>>,
    }

    impl Said {
        /// Every number at `x` and the moods with them.
        fn heard(sound: Vec<f32>, x: f32) -> Self {
            Said { energy: Some(x), danceability: Some(x), moods: Some([x; 5]), sound: Some(sound), ..Said::default() }
        }

        fn without(&self, s: Signal) -> Self {
            let mut t = self.clone();
            match s {
                Playlists => t.playlists.clear(),
                Artists => t.artists.clear(),
                Genres => t.genres.clear(),
                Tags => t.tags.clear(),
                Styles => t.styles.clear(),
                Sound => t.sound = None,
                Era => t.year = None,
                Tempo => t.bpm = None,
                Energy => t.energy = None,
                Danceability => t.danceability = None,
                Mood => t.moods = None,
                Tracks => {}
            }
            t
        }
    }

    /// A sound of unit length at this cosine to (1, 0, 0, 0).
    fn at(cos: f32) -> Vec<f32> {
        vec![cos, (1.0 - cos * cos).sqrt(), 0.0, 0.0]
    }

    /// One of the scenarios' libraries: its tracks by name, their words numbered per kind as a caller numbers them, the decades
    /// told so.
    struct Shelf {
        lib: Library,
        tracks: Vec<Track>,
        at: HashMap<String, usize>,
    }

    impl Shelf {
        fn of(fillers: Vec<Said>, cases: Vec<(String, Said)>) -> Self {
            let mut named: [HashMap<&'static str, u32>; WORDS.len()] = Default::default();
            let (mut tracks, mut sounds, mut at) = (Vec::new(), Vec::new(), HashMap::new());
            let all = fillers.into_iter().enumerate().map(|(i, s)| (format!("f{i}"), s)).chain(cases);
            for (name, s) in all {
                let mut words = |k: usize, list: &[&'static str]| -> Vec<u32> {
                    // A word on every track: the scenarios' "a genre every track of the library has" (SIM.5 (d)).
                    let every = (k == 2).then_some("every");
                    list.iter().copied().chain(every).map(|w| {
                        let next = named[k].len() as u32;
                        *named[k].entry(w).or_insert(next)
                    })
                    .collect()
                };
                tracks.push(Track {
                    playlists: words(0, &s.playlists),
                    artists: words(1, &s.artists),
                    genres: words(2, &s.genres),
                    tags: words(3, &s.tags),
                    styles: words(4, &s.styles),
                    year: s.year,
                    bpm: s.bpm,
                    energy: s.energy,
                    danceability: s.danceability,
                    moods: s.moods,
                });
                sounds.push(s.sound.map(Arc::from));
                at.insert(name, tracks.len() - 1);
            }
            let mut lib = Library::new(&tracks, sounds);
            for (k, kind) in WORDS.into_iter().enumerate() {
                lib = lib.untold(kind, named[k].iter().filter(|(w, _)| era_word(w)).map(|(_, &n)| n));
            }
            Shelf { lib, tracks, at }
        }

        fn places(&self, names: &[&str]) -> Vec<usize> {
            names.iter().map(|n| self.at[*n]).collect()
        }

        fn profile(&self, names: &[&str]) -> Profile {
            Profile::of(&self.lib, &self.places(names), |i| &self.tracks[i])
        }

        fn to_track(&self, seed: &Profile, name: &str) -> Signals {
            let i = self.at[name];
            seed.to_track(&self.lib, i, &self.tracks[i])
        }

        fn to_set(&self, seed: &Profile, names: &[&str]) -> Signals {
            seed.to_profile(&self.lib, &self.profile(names))
        }

        fn names_like(&self, start: &str) -> Vec<&str> {
            let mut names: Vec<&str> = self.at.keys().map(String::as_str).filter(|n| n.starts_with(start)).collect();
            names.sort_by_key(|n| self.at[*n]);
            names
        }
    }

    /// F-KINDS: `fillers` tracks, each its own artist, genre "Rock", no sound, its year, BPM, energy, danceability and every mood
    /// by four blocks; the tag "common" on the first `common`; every track has the genre "every" too. Then the scenarios' cases.
    fn f_kinds(fillers: usize, common: usize) -> Shelf {
        let filler = |i: usize| {
            let block = i % 4;
            let x = [0.0, 0.25, 0.75, 1.0][block];
            Said {
                artists: vec![Box::leak(format!("artist {i}").into_boxed_str())],
                genres: vec!["Rock"],
                tags: if i < common { vec!["common"] } else { Vec::new() },
                year: Some([1980, 1990, 2010, 2020][block]),
                bpm: Some([40.0, 80.0, 120.0, 160.0][block]),
                energy: Some(x),
                danceability: Some(x),
                moods: Some([x; 5]),
                ..Said::default()
            }
        };
        let v = at(1.0);
        let s1 = Said {
            playlists: vec!["p"],
            artists: vec!["a"],
            genres: vec!["g"],
            tags: vec!["t"],
            styles: vec!["y"],
            year: Some(2000),
            bpm: Some(100.0),
            ..Said::heard(v.clone(), 0.5)
        };
        let mut cases: Vec<(String, Said)> = vec![("s1".into(), s1.clone())];
        cases.extend(HAS.map(|s| (format!("c-{s:?}"), s1.without(s))));
        let s3 = Said { genres: vec!["g3"], year: Some(2000), bpm: Some(100.0), energy: Some(0.5), sound: Some(v.clone()), ..Said::default() };
        let r3 = Said { genres: vec!["g3"], year: Some(2010), bpm: Some(120.0), energy: Some(0.75), sound: Some(at(0.81)), ..Said::default() };
        let p3 = Said { genres: vec!["g3"], year: Some(2000), ..Said::default() };
        let heard = Said::heard(v.clone(), 0.5);
        let year = |y: u32| Said { year: Some(y), ..Said::default() };
        let h = Said { genres: vec!["hg"], styles: vec!["hs"], tags: vec!["80s", "common"], year: Some(2000), bpm: Some(100.0), ..Said::heard(v.clone(), 0.5) };
        let near = |cos: f32, genre: &'static str, style: &'static str| Said { genres: vec![genre], styles: vec![style], year: Some(2002), bpm: Some(104.0), ..Said::heard(at(cos), 0.55) };
        cases.extend([
            ("s2".into(), Said { artists: vec!["a2"], year: Some(2000), ..Said::default() }),
            ("c2".into(), Said { artists: vec!["a2"], year: Some(2010), ..s1.clone() }),
            ("c2-bare".into(), Said { artists: vec!["a2"], year: Some(2010), ..Said::default() }),
            ("s3".into(), s3.clone()),
            ("R3".into(), r3.clone()),
            ("P3".into(), p3.clone()),
            ("year".into(), year(2000)),
            ("year-bpm".into(), Said { bpm: Some(100.0), ..year(2000) }),
            ("every-year".into(), year(2000)),
            ("heard".into(), heard.clone()),
            ("heard-year".into(), Said { year: Some(2000), ..heard.clone() }),
            ("rafting".into(), Said { artists: vec!["rg"], genres: vec!["hip-hop/rap"], year: Some(2026), ..Said::default() }),
            ("year-80s".into(), Said { tags: vec!["80s"], ..year(2000) }),
            ("year-common".into(), Said { tags: vec!["common"], ..year(2000) }),
            ("heard-drone".into(), Said { styles: vec!["drone"], ..heard }),
            ("L".into(), Said { artists: vec!["l"], genres: vec!["w1"], ..year(2000) }),
            ("L1".into(), Said { artists: vec!["m"], genres: vec!["w2"], ..year(2000) }),
            ("L2".into(), Said { artists: vec!["l"], genres: vec!["w2"], ..year(2000) }),
            ("H".into(), h),
            ("H1".into(), near(0.60, "h1g", "h1s")),
            ("H2".into(), near(0.70, "h2g", "h2s")),
            ("H3".into(), near(0.60, "h3g", "hs")),
            ("H4".into(), Said { tags: vec!["80s"], ..near(0.60, "h4g", "h4s") }),
            ("H5".into(), Said { tags: vec!["common"], ..near(0.60, "h5g", "h5s") }),
        ]);
        for n in 0..10 {
            cases.push((format!("al-y{n}"), Said { artists: vec!["ay"], ..year(1998) }));
            cases.push((format!("al-s3-{n}"), s3.clone()));
            cases.push((format!("al-R3-{n}"), r3.clone()));
            cases.push((format!("al-P3-{n}"), p3.clone()));
        }
        Shelf::of((0..fillers).map(filler).collect(), cases)
    }

    /// F-KINDS' reaches, its size and "common"'s share, asserted before anything is read from it.
    fn reaches_as_written(k: &Shelf, big: bool) {
        assert_eq!(k.lib.reach[YEAR], 20.0, "year");
        assert_eq!(k.lib.reach[1], 40.0, "BPM");
        for (n, reach) in k.lib.reach[2..].iter().enumerate() {
            assert_eq!(*reach, 0.5, "energy, danceability and each mood: {n}");
        }
        let n = k.tracks.len() as f64;
        assert_eq!(n >= COMMON_FROM, big, "{n} tracks");
        let common = k.at.iter().filter(|(name, i)| name.starts_with('f') && !k.tracks[**i].tags.is_empty()).count() as f64;
        assert!(common / n > 0.25, "\"common\" is on more than a quarter: {common} of {n}");
    }

    /// Every bit of what a comparison says.
    fn bits(s: &Signals) -> String {
        format!("{s:?}")
    }

    fn near(a: f64, b: f64, by: f64) -> bool {
        (a - b).abs() < by
    }

    /// SIM.1: a signal the seed has and a candidate lacks reads not alike (0), never skipped.
    #[test]
    fn a_signal_the_seed_has_and_a_candidate_lacks_reads_not_alike() {
        let k = f_kinds(1000, 300);
        reaches_as_written(&k, true);
        let seed = k.profile(&["s1"]);
        let alone = [seed.clone()];
        let mut together = Together::of(&k.lib, &alone);
        for lacking in HAS {
            let twin = format!("c-{lacking:?}");
            let t = k.to_track(&seed, &twin);
            assert_eq!(t.get(lacking), Some(0.0), "{lacking:?}, to_track");
            for other in HAS.into_iter().filter(|o| *o != lacking) {
                assert!(near(t.get(other).unwrap(), 1.0, 1e-9), "{lacking:?}: {other:?} {:?}", t.get(other));
            }
            assert!(near(t.score(&Weights::EQUAL), 10.0 / 11.0, 1e-9), "{lacking:?}: {}", t.score(&Weights::EQUAL));
            let p = k.to_set(&seed, &[&twin]);
            assert_eq!((p.get(lacking), p.get(Tracks)), (Some(0.0), Some(0.0)), "{lacking:?}, to_profile");
            assert!(near(p.score(&Weights::EQUAL), 10.0 / 12.0, 1e-9), "{lacking:?}: {}", p.score(&Weights::EQUAL));
            let i = k.at[&twin];
            together.to_track(i, &k.tracks[i], |_, signals| assert_eq!(bits(&signals), bits(&t), "{lacking:?}, Together"));
        }
    }

    /// SIM.2: signals the seed lacks are left out; the seed's side decides, so A against B can differ from B against A.
    #[test]
    fn what_the_seed_lacks_is_left_out_and_the_seeds_side_decides() {
        let k = f_kinds(1000, 300);
        reaches_as_written(&k, true);
        let (s2, c2) = (k.profile(&["s2"]), k.profile(&["c2"]));
        let t = k.to_track(&s2, "c2");
        assert_eq!((t.get(Artists), t.get(Era)), (Some(1.0), Some(0.5)));
        assert!(HAS.into_iter().filter(|s| ![Artists, Era].contains(s)).all(|s| t.get(s).is_none()), "{t:?}");
        assert!(near(t.score(&Weights::EQUAL), 0.75, 1e-9));
        let p = k.to_set(&s2, &["c2"]);
        assert_eq!((p.get(Tracks), p.get(Artists), p.get(Era)), (Some(0.0), Some(1.0), Some(0.5)));
        assert!(near(p.score(&Weights::EQUAL), 0.5, 1e-9));
        // What c2 has and s2 lacks changes nothing.
        assert_eq!(bits(&t), bits(&k.to_track(&s2, "c2-bare")));
        assert_eq!(bits(&p), bits(&k.to_set(&s2, &["c2-bare"])));
        // The other way: c2's eleven signals, s2 alike on its artist and half on its year.
        let back = k.to_track(&c2, "s2");
        assert_eq!((back.get(Artists), back.get(Era)), (Some(1.0), Some(0.5)));
        assert!(near(back.score(&Weights::EQUAL), 1.5 / 11.0, 1e-9), "{}", back.score(&Weights::EQUAL));
        assert!(near(k.to_set(&c2, &["s2"]).score(&Weights::EQUAL), 1.5 / 12.0, 1e-9));
    }

    /// SIM.3: a sparse candidate no longer beats a rich right one.
    #[test]
    fn a_sparse_candidate_never_beats_a_rich_right_one() {
        let k = f_kinds(1000, 300);
        reaches_as_written(&k, true);
        let s3 = k.profile(&["s3"]);
        let (r, p) = (k.to_track(&s3, "R3"), k.to_track(&s3, "P3"));
        assert!(near(r.score(&Weights::EQUAL), 0.6, 1e-6) && near(p.score(&Weights::EQUAL), 0.4, 1e-6), "{r:?} {p:?}");
        let (r, p) = (k.to_set(&s3, &["R3"]), k.to_set(&s3, &["P3"]));
        assert!(near(r.score(&Weights::EQUAL), 0.5, 1e-6) && near(p.score(&Weights::EQUAL), 1.0 / 3.0, 1e-6), "{r:?} {p:?}");
        // As albums of ten: Similar albums' weights, Tracks out; s3 has no artist, so Artists is left out.
        let albums = Weights::EQUAL.with(Tracks, 0.0);
        let s3 = k.profile(&k.names_like("al-s3-"));
        let (r, p) = (k.to_set(&s3, &k.names_like("al-R3-")), k.to_set(&s3, &k.names_like("al-P3-")));
        assert!(near(r.score(&albums), 0.6, 1e-6) && near(p.score(&albums), 0.4, 1e-6), "{r:?} {p:?}");
        assert!(r.shown(&albums).is_some() && p.shown(&albums).is_some(), "both at or above the floor, alike on the genre");
    }

    /// SIM.4: never lifted by a lack, for every seed and candidate of a library of words of many rarities, numbers, moods and
    /// sounds, some missing; each track has a twin lacking each signal, in the same library.
    #[test]
    fn no_candidate_scores_higher_for_lacking_what_its_seed_has() {
        let base: Vec<Track> = (0..200u32)
            .map(|n| Track {
                playlists: if n % 3 == 0 { Vec::new() } else { vec![n % 4, 10 + n % 7] },
                genres: (0..n % 9).map(|g| (g * 7 + n % 3) % 41).collect(),
                artists: vec![n % 13, n % 13],
                tags: if n % 4 == 0 { Vec::new() } else { vec![n % 5, 1000 + n % 2] },
                styles: if n % 5 == 0 { Vec::new() } else { vec![n % 6] },
                year: (n % 3 != 0).then_some(1970 + n % 50),
                bpm: (n % 2 == 0).then_some(80.0 + (n % 90) as f32),
                energy: (n % 7 != 0).then_some((n % 10) as f32 / 10.0),
                danceability: (n % 3 != 1).then_some((n % 11) as f32 / 11.0),
                moods: (n % 5 != 0).then_some([0.1, 0.2, (n % 7) as f32 / 7.0, 0.4, 0.5]),
            })
            .collect();
        let sounds: Vec<Option<Arc<[f32]>>> = (0..200u64).map(|n| (n % 6 != 0).then(|| heard(n, 16).into())).collect();
        let strip = |t: &Track, s: Signal| -> Track {
            let mut t = t.clone();
            match s {
                Playlists => t.playlists.clear(),
                Artists => t.artists.clear(),
                Genres => t.genres.clear(),
                Tags => t.tags.clear(),
                Styles => t.styles.clear(),
                Era => t.year = None,
                Tempo => t.bpm = None,
                Energy => t.energy = None,
                Danceability => t.danceability = None,
                Mood => t.moods = None,
                Sound | Tracks => {}
            }
            t
        };
        // The twins of every track lacking each signal, a block a signal, in the tracks' order.
        let twin = |k: usize, i: usize| 200 * (1 + k) + i;
        let mut tracks = base.clone();
        let mut all_sounds = sounds.clone();
        for s in HAS {
            tracks.extend(base.iter().map(|t| strip(t, s)));
            all_sounds.extend(sounds.iter().map(|v| if s == Sound { None } else { v.clone() }));
        }
        let lib = Library::new(&tracks, all_sounds);
        let has = |set: &[usize], s: Signal| -> bool {
            set.iter().any(|&i| {
                let t = &tracks[i];
                match s {
                    Playlists => !t.playlists.is_empty(),
                    Artists => !t.artists.is_empty(),
                    Genres => !t.genres.is_empty(),
                    Tags => !t.tags.is_empty(),
                    Styles => !t.styles.is_empty(),
                    Sound => lib.sound(i).is_some(),
                    Era => t.year.is_some(),
                    Tempo => t.bpm.is_some(),
                    Energy => t.energy.is_some(),
                    Danceability => t.danceability.is_some(),
                    Mood => t.moods.is_some(),
                    Tracks => false,
                }
            })
        };
        // Seeds among the first hundred, candidates among the others: a set and its twin set share no track.
        let mut seeds: Vec<Vec<usize>> = (0..20).map(|n| vec![n * 5]).collect();
        seeds.extend((0..10).map(|j| (j * 7..j * 7 + 2 + j * 2).collect::<Vec<usize>>()));
        let candidates: Vec<Vec<usize>> = (0..10).map(|j| (100 + j * 9..100 + j * 9 + 1 + j).collect()).collect();
        let weights = [
            Weights::EQUAL,
            Weights::ranked(&[Sound, Genres]),
            Weights::ranked(&[Era]),
            Weights::EQUAL.with(Artists, 2.0),
            Weights::EQUAL.with(Tracks, 0.0),
        ];
        let checked = |seed: &[usize], s: Signal, what: &str, original: &Signals, lacking: &Signals| {
            for w in &weights {
                let (was, now) = (original.score(w), lacking.score(w));
                if has(seed, s) {
                    assert!(now <= was + 1e-12, "{what}: {seed:?} lacking {s:?} by {w:?}: {now} over {was}");
                } else {
                    assert_eq!(now.to_bits(), was.to_bits(), "{what}: {seed:?}, its seed lacking {s:?}");
                }
            }
        };
        for seed in &seeds {
            let p = Profile::of(&lib, seed, |i| &tracks[i]);
            let alone = [p.clone()];
            let mut together = Together::of(&lib, &alone);
            for (k, s) in HAS.into_iter().enumerate() {
                for c in 100..200 {
                    let (original, lacking) = (p.to_track(&lib, c, &tracks[c]), p.to_track(&lib, twin(k, c), &tracks[twin(k, c)]));
                    checked(seed, s, "to_track", &original, &lacking);
                    together.to_track(twin(k, c), &tracks[twin(k, c)], |_, t| checked(seed, s, "Together", &original, &t));
                }
                for set in &candidates {
                    let theirs: Vec<usize> = set.iter().map(|&c| twin(k, c)).collect();
                    let (original, lacking) =
                        (p.to_profile(&lib, &Profile::of(&lib, set, |i| &tracks[i])), p.to_profile(&lib, &Profile::of(&lib, &theirs, |i| &tracks[i])));
                    checked(seed, s, "to_profile", &original, &lacking);
                }
            }
        }
    }

    /// SIM.5: what a seed is known by (rule 3, T-SIM-2): the kinds the weights count, a word kind through a telling word.
    #[test]
    fn a_seed_known_by_fewer_than_two_kinds_is_too_little_known() {
        let (k, small) = (f_kinds(1000, 300), f_kinds(200, 70));
        reaches_as_written(&k, true);
        reaches_as_written(&small, false);
        let (equal, sound) = (Weights::EQUAL, Weights::ranked(&[Sound]));
        let album = k.names_like("al-y");
        let rows: [(&str, &Shelf, Vec<&str>, Weights, usize, bool); 17] = [
            ("(a) a year alone", &k, vec!["year"], equal, 1, true),
            ("(b) year and BPM", &k, vec!["year-bpm"], equal, 2, false),
            ("(c) artist and year: the artist is no kind", &k, vec!["s2"], equal, 1, true),
            ("(d) a genre every track has, and a year", &k, vec!["every-year"], equal, 1, true),
            ("(e) heard, nothing else", &k, vec!["heard"], equal, 1, true),
            ("(f) heard, and a year", &k, vec!["heard-year"], equal, 2, false),
            ("(g) an album known by its artist and year: tracks are no kind", &k, album, equal, 1, true),
            ("(h) the empty seed", &k, vec![], equal, 0, true),
            ("(i) Rafting Goods: artist, genre word, year", &k, vec!["rafting"], equal, 2, false),
            ("(j) heard, by Sound alone", &k, vec!["heard"], sound, 1, false),
            ("(k) Rafting Goods, by Sound alone", &k, vec!["rafting"], sound, 0, true),
            ("(l) Rafting Goods, by Sound and Genres", &k, vec!["rafting"], Weights::ranked(&[Sound, Genres]), 1, true),
            ("(m) heard and a year, by Sound and Era", &k, vec!["heard-year"], Weights::ranked(&[Sound, Era]), 2, false),
            ("(n) a year and \"80s\": a decade is no kind", &k, vec!["year-80s"], equal, 1, true),
            ("(o) a year and a word on over a quarter of 1,000 songs and more", &k, vec!["year-common"], equal, 1, true),
            ("(o') the same under 1,000 songs: the quarter does not apply", &small, vec!["year-common"], equal, 2, false),
            ("(p) heard, and a style: styles are no AudioMuse", &k, vec!["heard-drone"], equal, 2, false),
        ];
        for (what, shelf, seed, w, kinds, too_little) in rows {
            let p = shelf.profile(&seed);
            assert_eq!((p.kinds(&w), p.too_little(&w)), (kinds, too_little), "{what}");
        }
    }

    /// SIM.7: the floor, one constant, read on the raw score (0.29996 would show as 30.0).
    #[test]
    fn nothing_below_the_floor_is_shown() {
        assert_eq!(ALIKE_FLOOR, 0.30);
        let by = Weights::ranked(&[Genres]);
        for (x, shown) in [(0.2999, false), (0.29996, false), (0.30, true), (0.3001, true)] {
            let s = Signals::default().alike_on(Genres, x);
            assert_eq!(s.shown(&by), shown.then_some(x), "{x}");
        }
        assert_eq!((0.29996f64 * 1000.0).round() / 10.0, 30.0, "what a list would show of it");
    }

    /// SIM.8: something telling must match; numbers alone never do (rule 6), whatever Similar by names.
    #[test]
    fn something_telling_must_match() {
        let (k, small) = (f_kinds(1000, 300), f_kinds(200, 70));
        reaches_as_written(&k, true);
        reaches_as_written(&small, false);
        let (albums, inserts, era) = (Weights::EQUAL.with(Tracks, 0.0), Weights::EQUAL.with(Artists, 2.0), Weights::ranked(&[Era]));
        let numbers = 0.9 * 5.0;
        // (pair, read as, the score, or none where only "above"; tags above 0; shown)
        struct Row<'a> {
            shelf: &'a Shelf,
            seed: &'a str,
            to: &'a str,
            set: bool,
            w: Weights,
            score: Option<f64>,
            shown: bool,
        }
        let row = |seed, to, set, w, score: Option<f64>, shown| Row { shelf: &k, seed, to, set, w, score, shown };
        let rows = [
            row("L", "L1", true, albums, Some(1.0 / 3.0), false),
            row("L", "L2", true, albums, Some(2.0 / 3.0), true),
            row("L", "L1", false, inserts, Some(0.25), false),
            row("L", "L2", false, inserts, Some(0.75), true),
            row("H", "H1", true, albums, Some(numbers / 9.0), false),
            row("H", "H2", true, albums, Some((0.08 / 0.38 + numbers) / 9.0), true),
            row("H", "H3", true, albums, Some((1.0 + numbers) / 9.0), true),
            row("H", "H4", true, albums, None, false),
            row("H", "H5", true, albums, None, false),
            Row { shelf: &small, ..row("H", "H5", true, albums, None, true) },
            Row { shelf: &small, ..row("H", "H4", true, albums, None, false) },
            row("L", "L2", true, era, Some(1.0), true),
            row("L", "L1", true, era, Some(1.0), false),
            row("H", "H3", true, era, Some(0.9), true),
        ];
        for r in rows {
            let seed = r.shelf.profile(&[r.seed]);
            let s = if r.set { r.shelf.to_set(&seed, &[r.to]) } else { r.shelf.to_track(&seed, r.to) };
            let what = format!("{} -> {} by {:?}", r.seed, r.to, r.w);
            match r.score {
                Some(x) => assert!(near(s.score(&r.w), x, 1e-6), "{what}: {}", s.score(&r.w)),
                None => {
                    assert!(s.get(Tags).is_some_and(|t| t > 0.0), "{what}: tags above 0");
                    assert!(near(s.score(&r.w), (s.get(Tags).unwrap() + numbers) / 9.0, 1e-6), "{what}");
                }
            }
            assert_eq!(s.shown(&r.w).is_some(), r.shown, "{what}: {}", s.score(&r.w));
        }
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
