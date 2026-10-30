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

use std::collections::HashMap;

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
    /// Each track's "sounds like" vector, of unit length, where AudioMuse heard it.
    sounds: Vec<Option<Box<[f32]>>>,
}

impl Library {
    /// `sounds`: by the tracks' places, as `tracks`; any length (missing ones unheard), any length of vector (unit
    /// length is made here).
    pub fn new<'a>(tracks: impl IntoIterator<Item = &'a Track>, sounds: Vec<Option<Vec<f32>>>) -> Self {
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
                (len > 0.0 && len.is_finite()).then(|| v.into_iter().map(|x| x / len).collect())
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
    counts: [HashMap<u32, f64>; WORDS.len()],
    /// Per kind of word, its tf-idf weights (the share of its tracks having the word times its rarity) and their length.
    weights: [HashMap<u32, f64>; WORDS.len()],
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
        let mut weight: HashMap<usize, f64> = HashMap::new();
        for &(i, w) in set.iter().filter(|(_, w)| w.is_finite() && *w > 0.0) {
            *weight.entry(i).or_default() += w;
        }
        let mut members: Vec<usize> = weight.keys().copied().collect();
        members.sort_unstable();
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
    pub fn artists(&self) -> &HashMap<u32, f64> {
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
        let moods: Vec<f64> = close[4..].iter().flatten().copied().collect();
        if !moods.is_empty() {
            out.set(Signal::Mood, moods.iter().sum::<f64>() / moods.len() as f64);
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

pub(crate) fn unique(items: &[u32]) -> impl Iterator<Item = u32> + '_ {
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
        let lib = Library::new(&tracks, vec![Some(vec![1.0, 0.0]), None, Some(vec![0.0, 1.0])]);
        let seed = Profile::of(&lib, &[0], |i| &tracks[i]);
        let unheard = seed.to_track(&lib, 1, &tracks[1]);
        assert_eq!((unheard.get(Signal::Energy), unheard.get(Signal::Sound), unheard.get(Signal::Tracks)), (None, None, None));
        assert_eq!(unheard.score(&Weights::EQUAL), 1.0, "alike in all that is known");
        assert_eq!(seed.to_track(&lib, 2, &tracks[2]).score(&Weights::EQUAL), 0.0);
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
