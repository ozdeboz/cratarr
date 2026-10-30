//! Cratarr's rules for what plays and in what order (the owner, 2026-10-23: what plays is the server's choice, a queue
//! always has an order, a queue holds 500 tracks at most). One implementation for the server and for players that play
//! on their own (the page through WebAssembly, offline, a device's own library), so every player keeps the same rules.
//!
//! Pure: no I/O, no clock, no randomness of its own. The caller seeds the [`Rng`]: a seed gives the same answer
//! everywhere (tests/vectors holds cases with their answers, for the other implementations to check against).
//! Everything works on plain data: places in a list, a track's lane, a queue entry's key; the caller keeps the tracks.
//!
//! - **Picking** ([`pick`]): which of a collection's tracks go in a queue, at most [`QUEUE_MOST`], and which plays
//!   first.
//! - **The queue after the playing track** (decisions 0033 and 0036, amended): where tracks added by hand go
//!   ([`add_place`]), a shuffle once ([`shuffle_order`]), clearing ([`upcoming_start`]) and Undo of both
//!   ([`restore_order`]).
//! - **A let-go's brake** ([`brake`], decision 0075): its length, speed and way, for the page through WebAssembly too.

use serde::{Deserialize, Serialize};

pub mod brake;
#[cfg(target_arch = "wasm32")]
mod wasm;

/// The most tracks one play puts in a queue (the owner, 2026-10-23).
pub const QUEUE_MOST: usize = 500;

/// A small generator (xorshift64), seeded by the caller: an even draw and a fair shuffle need no better, and the same
/// seed gives the same numbers on every platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng(u64);

impl Rng {
    /// Any seed will do, 0 too: it is spread first (SplitMix64), so seeds near each other start far apart.
    pub fn seeded(seed: u64) -> Self {
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        // Xorshift never leaves 0.
        Self(if z == 0 { 0x9E37_79B9_7F4A_7C15 } else { z })
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// From 0 up to (not including) `n`; 0 when `n` is 0.
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }

    /// From 0 up to (not including) 1.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// In place, every order as likely (Fisher–Yates, from the end).
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            items.swap(i, self.below(i + 1));
        }
    }
}

/// How a collection is played: Play (its order), Shuffle, Play next, Add to queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum How {
    Play,
    Shuffle,
    Next,
    Last,
}

/// The tracks a play queues: places in the collection, in queue order, and the place in the queue that plays first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Picked {
    pub chosen: Vec<usize>,
    pub index: usize,
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

/// Which of a collection's `n` playable tracks go in the queue, and the one that plays first. `at`: the track asked to
/// play first (its place in the collection); `most`: at most this many (at least 1; [`QUEUE_MOST`] for a play).
///
/// - Play, Play next, Add to queue: `most` in the collection's order, from `at` on; when fewer than `most` follow it,
///   those before it fill up the queue, so a collection that small is queued whole from that track.
/// - Shuffle: `most` drawn evenly from all `n`, in a random order; `at` first, when given.
pub fn pick(n: usize, how: How, at: Option<usize>, most: usize, rng: &mut Rng) -> Picked {
    let at = at.filter(|&a| a < n);
    let most = most.max(1);
    match how {
        How::Shuffle => match at {
            Some(a) => {
                let rest = draw((0..n).filter(|&i| i != a).collect(), most - 1, rng);
                Picked { chosen: std::iter::once(a).chain(rest).collect(), index: 0 }
            }
            None => Picked { chosen: draw((0..n).collect(), most, rng), index: 0 },
        },
        How::Play | How::Next | How::Last => {
            let a = at.unwrap_or(0);
            let start = a.min(n.saturating_sub(most));
            Picked { chosen: (start..n.min(start + most)).collect(), index: a - start }
        }
    }
}

/// A queue entry's lane (decisions 0033 and 0036, amended): the queue's own tracks (the album, playlist or collection
/// played), added by hand (Play next, Add to queue), Radio's inserts, Autoplay's tail, or one this does not know.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lane {
    Own,
    Next,
    Radio,
    Autoplay,
    Other,
}

impl Lane {
    /// From a track's `lane` as players hold it: none for the queue's own tracks.
    pub fn of(lane: Option<&str>) -> Self {
        match lane {
            None => Lane::Own,
            Some("next") => Lane::Next,
            Some("radio") => Lane::Radio,
            Some("autoplay") => Lane::Autoplay,
            Some(_) => Lane::Other,
        }
    }

    /// As players hold it (`Other` has no name of its own: keep the track's).
    pub fn name(self) -> Option<&'static str> {
        match self {
            Lane::Own | Lane::Other => None,
            Lane::Next => Some("next"),
            Lane::Radio => Some("radio"),
            Lane::Autoplay => Some("autoplay"),
        }
    }
}

/// Where the tracks after the playing one start: right after it, or the whole queue when nothing plays. A clear keeps
/// the queue up to there.
pub fn upcoming_start(current: Option<usize>, len: usize) -> usize {
    current.map_or(0, |c| c + 1).min(len)
}

fn run(lanes: &[Lane], from: usize, lane: Lane) -> usize {
    lanes[from.min(lanes.len())..].iter().take_while(|&&l| l == lane).count()
}

/// How many tracks added by hand are still to come: the run of them right after the playing one.
pub fn added_ahead(lanes: &[Lane], current: Option<usize>) -> usize {
    current.map_or(0, |c| run(lanes, c + 1, Lane::Next))
}

/// Tracks added by hand: Play next (the newest first) or Add to queue (after those added before).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ByHand {
    Next,
    Last,
}

/// The place in the queue tracks added by hand go: right after the playing one (`next`), or after the run added by hand
/// before (`last`); ahead of the queue's own tracks either way. With nothing playing, at the end.
pub fn add_place(lanes: &[Lane], current: Option<usize>, by: ByHand) -> usize {
    let len = lanes.len();
    match current {
        None => len,
        Some(c) => match by {
            ByHand::Next => (c + 1).min(len),
            ByHand::Last => (c + 1 + added_ahead(lanes, current)).min(len),
        },
    }
}

/// Shuffle, once (the owner, 2026-10-23: a queue always has an order; shuffle is an action, not a state): the new order
/// of the tracks after the playing one, as places in the queue. The playing track stays; what was added by hand stays
/// right after it, Radio's inserts for it right after those, Autoplay's tail last, each in its order; only the queue's
/// own tracks are mixed (and tracks of another lane further on keep their order, after them). None when nothing plays
/// or fewer than two own tracks are to come: nothing to mix.
pub fn shuffle_order(lanes: &[Lane], current: Option<usize>, rng: &mut Rng) -> Option<Vec<usize>> {
    let c = current.filter(|&c| c < lanes.len())?;
    let from = c + 1;
    let ahead = from + run(lanes, from, Lane::Next);
    let inserts = ahead + run(lanes, ahead, Lane::Radio);
    let rest = inserts..lanes.len();
    let mut own: Vec<usize> = rest.clone().filter(|&i| lanes[i] == Lane::Own).collect();
    if own.len() < 2 {
        return None;
    }
    rng.shuffle(&mut own);
    let others = rest.clone().filter(|&i| !matches!(lanes[i], Lane::Own | Lane::Autoplay));
    let tail = rest.filter(|&i| lanes[i] == Lane::Autoplay);
    Some((from..inserts).chain(own).chain(others).chain(tail).collect())
}

/// Undo of a shuffle or a clear: which of the tracks that were to come (`before`, by their entries' keys) come back, as
/// places in `before`, in its order. They take the place of whatever is to come now (what came meanwhile: Autoplay's);
/// those played since (in `queue` at or before the playing one, `current`) do not come back.
pub fn restore_order<K: PartialEq>(queue: &[K], current: Option<usize>, before: &[K]) -> Vec<usize> {
    let played = &queue[..upcoming_start(current, queue.len())];
    (0..before.len()).filter(|&i| !played.contains(&before[i])).collect()
}

/// The items at these places, in this order (to apply [`pick`], [`shuffle_order`] or [`restore_order`]).
pub fn take<T: Clone>(items: &[T], places: &[usize]) -> Vec<T> {
    places.iter().filter_map(|&i| items.get(i).cloned()).collect()
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
            let p = pick(n, How::Shuffle, None, QUEUE_MOST, &mut rng);
            assert_eq!((p.chosen.len(), p.index), (QUEUE_MOST, 0));
            assert!(distinct(&p.chosen) && p.chosen.iter().all(|&i| i < n));
            for &i in &p.chosen {
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
        let p = pick(10_000, How::Shuffle, None, QUEUE_MOST, &mut Rng::seeded(3));
        let rising = p.chosen.windows(2).filter(|w| w[0] < w[1]).count();
        assert!((150..350).contains(&rising), "{rising} of 499 rising");
    }

    #[test]
    fn a_small_collection_shuffles_whole() {
        let mut p = pick(300, How::Shuffle, None, QUEUE_MOST, &mut Rng::seeded(1));
        assert_eq!(p.index, 0);
        p.chosen.sort_unstable();
        assert_eq!(p.chosen, (0..300).collect::<Vec<_>>());
        assert_eq!(pick(0, How::Shuffle, None, QUEUE_MOST, &mut Rng::seeded(1)).chosen, Vec::<usize>::new());
    }

    #[test]
    fn a_shuffle_from_a_track_plays_it_first_and_draws_the_rest() {
        let p = pick(10_000, How::Shuffle, Some(9_876), QUEUE_MOST, &mut Rng::seeded(5));
        assert_eq!((p.chosen.len(), p.chosen[0], p.index), (QUEUE_MOST, 9_876, 0));
        assert!(distinct(&p.chosen));
        let p = pick(3, How::Shuffle, Some(1), QUEUE_MOST, &mut Rng::seeded(5));
        assert_eq!((p.chosen.len(), p.chosen[0]), (3, 1));
    }

    fn played(chosen: std::ops::Range<usize>, index: usize) -> Picked {
        Picked { chosen: chosen.collect(), index }
    }

    #[test]
    fn play_takes_the_first_ones_in_order_at_most_the_cap() {
        let mut rng = Rng::seeded(1);
        assert_eq!(pick(10_000, How::Play, None, QUEUE_MOST, &mut rng), played(0..500, 0));
        assert_eq!(pick(42, How::Play, None, QUEUE_MOST, &mut rng), played(0..42, 0));
        assert_eq!(pick(10_000, How::Next, None, QUEUE_MOST, &mut rng), played(0..500, 0));
        assert_eq!(pick(10_000, How::Last, None, QUEUE_MOST, &mut rng).chosen.len(), QUEUE_MOST);
        // A smaller limit (Home Assistant's, a voice assistant's).
        assert_eq!(pick(10_000, How::Play, None, 100, &mut rng), played(0..100, 0));
        assert_eq!(pick(10_000, How::Shuffle, None, 100, &mut rng).chosen.len(), 100);
    }

    #[test]
    fn play_from_a_track_queues_from_it_and_fills_up_with_those_before() {
        let mut rng = Rng::seeded(1);
        // Far from the end: from it on.
        assert_eq!(pick(1_000, How::Play, Some(300), QUEUE_MOST, &mut rng), played(300..800, 0));
        // Near the end: those before it fill up the queue, and it plays first.
        assert_eq!(pick(1_000, How::Play, Some(900), QUEUE_MOST, &mut rng), played(500..1_000, 400));
        // A small collection: whole, from that track.
        assert_eq!(pick(50, How::Play, Some(20), QUEUE_MOST, &mut rng), played(0..50, 20));
        // A track not in it: from the start.
        assert_eq!(pick(50, How::Play, Some(99), QUEUE_MOST, &mut rng), played(0..50, 0));
    }

    #[test]
    fn the_same_seed_gives_the_same_answer() {
        let a = pick(5_000, How::Shuffle, None, QUEUE_MOST, &mut Rng::seeded(42));
        let b = pick(5_000, How::Shuffle, None, QUEUE_MOST, &mut Rng::seeded(42));
        assert_eq!(a, b);
        assert_ne!(a, pick(5_000, How::Shuffle, None, QUEUE_MOST, &mut Rng::seeded(43)));
        let mut r = Rng::seeded(0);
        assert_ne!(r.next_u64(), 0, "a seed of 0 is no stuck generator");
        let u = Rng::seeded(9).unit();
        assert!((0.0..1.0).contains(&u));
    }

    use Lane::{Autoplay as A, Next as N, Other as O, Own as W, Radio as R};

    #[test]
    fn a_shuffle_once_mixes_only_the_queues_own_tracks() {
        // Playing 1; then two added by hand, Radio's two inserts, own tracks with an insert of a later one between,
        // Autoplay's tail.
        let lanes = [W, W, N, N, R, R, W, W, W, O, W, W, A, A];
        let order = shuffle_order(&lanes, Some(1), &mut Rng::seeded(11)).unwrap();
        assert_eq!(order.len(), lanes.len() - 2, "every track to come, once");
        assert!(distinct(&order));
        assert_eq!(&order[..4], [2, 3, 4, 5], "hand-added, then the inserts, in place");
        let mut own = order[4..9].to_vec();
        assert_ne!(own, [6, 7, 8, 10, 11], "mixed (seed 11)");
        own.sort_unstable();
        assert_eq!(own, [6, 7, 8, 10, 11], "the own tracks, all of them");
        assert_eq!(&order[9..], [9, 12, 13], "another lane after them, Autoplay's tail last");
        // The tracks before the playing one are never touched.
        assert!(order.iter().all(|&i| i > 1));
    }

    #[test]
    fn a_shuffle_once_has_nothing_to_mix_without_two_own_tracks_to_come() {
        let mut rng = Rng::seeded(1);
        assert_eq!(shuffle_order(&[W, W, W], None, &mut rng), None, "nothing plays");
        assert_eq!(shuffle_order(&[W, W, N, A, A], Some(0), &mut rng), None, "one own track to come");
        assert_eq!(shuffle_order(&[W, W, W], Some(2), &mut rng), None, "the last plays");
        assert_eq!(shuffle_order(&[W], Some(5), &mut rng), None, "a place past the end");
        assert!(shuffle_order(&[W, W, W], Some(0), &mut rng).is_some());
    }

    #[test]
    fn tracks_added_by_hand_go_ahead_of_the_queues_own() {
        let lanes = [W, W, N, N, W, A];
        assert_eq!(add_place(&lanes, Some(1), ByHand::Next), 2, "right after the playing one");
        assert_eq!(add_place(&lanes, Some(1), ByHand::Last), 4, "after those added before");
        assert_eq!(add_place(&lanes, Some(0), ByHand::Last), 1, "none added right after this one");
        assert_eq!(add_place(&lanes, None, ByHand::Next), 6, "nothing plays: at the end");
        assert_eq!(add_place(&[W], Some(0), ByHand::Last), 1);
        assert_eq!(added_ahead(&lanes, Some(1)), 2);
        assert_eq!(added_ahead(&lanes, None), 0);
    }

    #[test]
    fn a_clear_keeps_the_playing_track_and_undo_puts_back_what_was_to_come() {
        assert_eq!(upcoming_start(Some(2), 10), 3);
        assert_eq!(upcoming_start(None, 10), 0, "nothing plays: all of it");
        assert_eq!(upcoming_start(Some(9), 5), 5);
        // Cleared after b; Autoplay brought x and y, and x plays: all of it back, in place of y.
        let before = ["c", "d", "e"];
        assert_eq!(restore_order(&["a", "b", "x", "y"], Some(2), &before), [0, 1, 2]);
        // Shuffled after b; d and c have played since: e comes back, as it was.
        assert_eq!(restore_order(&["a", "b", "d", "c", "e"], Some(3), &before), [2]);
        assert_eq!(take(&before, &[2, 0]), ["e", "c"]);
        assert_eq!(take(&before, &[7]), Vec::<&str>::new(), "a place not there is left out");
    }
}
