//! A record's tracklist paired one for one with a copy's or a folder's files (PAIR01; DOMAIN-CORE §12, B.3, D.3): the pairing
//! with the most pairs, then the fewest by title alone, then the least total difference in length, found by `pathfinding`'s
//! `kuhn_munkres_min` over whole milliseconds. One function for Soulseek's folders and for the holding's copies (the store's
//! `copies_of` sets each copy's `paired` by it); the device never pairs, it reads what was paired.
//!
//! - **The caller says what may pair** (`fits`, one verdict per track and file): its title rule (X3's, the one rule) with
//!   [`fit`], the one length rule. Here are the tiers, the ties and the order, and nothing else is read (PAIR01.8).
//! - **Costs come in three tiers** ([`tiers`]), n being the most pairs one solve can make (the shorter side): a length within
//!   [`LENGTH_SLACK_MS`] costs its difference, 0 to 2 000; a pair by title alone costs n × 2 000 + 1, more than every difference
//!   together (T2); a forbidden pair n times that + 1, more than all the rest together. The solve's total stays inside `i64`.
//!   A forbidden pair is the solve's, never the answer's: it is dropped after, so `assign` gives the most pairs and whether
//!   every track is paired is the caller's check (T5).
//! - **Ties** go to the pairs whose files stand at their tracks' places (disc and number), then to the files' keys (their
//!   paths) in byte order, never to the order the files came in (T1). The place never outweighs a title and a length.
//! - **Lengths are whole milliseconds** (T3, T4): one slack, 2 000 ms inclusive; seconds become milliseconds by a checked
//!   multiply ([`ms_of_secs`]); a length no file can have ([`possible`]) is never within the slack.

use pathfinding::prelude::{Matrix, kuhn_munkres_min};
use serde::{Deserialize, Serialize};

/// The one slack (T3): two lengths this far apart or nearer are one length. Soulseek's folders, the gate and the holding read
/// this one (PAIR01.11).
pub const LENGTH_SLACK_MS: i64 = 2_000;

/// A track of the list, or a file: its place where known (no disc is disc 1), a key giving one fixed order (a file's path),
/// its length in whole milliseconds where known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Side<'a> {
    pub disc: Option<u32>,
    pub number: Option<u32>,
    pub key: &'a str,
    pub length_ms: Option<i64>,
}

/// What one track and one file are to each other: never a pair; a pair by title alone (a length unknown); or a pair whose
/// lengths lie this many whole milliseconds apart, 0 to [`LENGTH_SLACK_MS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    No,
    ByTitleAlone,
    Within(i64),
}

/// A length a track can have (T4): none below 0, and none at the ceiling a saturated or wrapped reading lands on.
pub fn possible(ms: i64) -> bool {
    (0..i64::MAX).contains(&ms)
}

/// Whole seconds (slskd's, as given) as milliseconds, by a checked multiply (T4): none where `i64` can't hold them, never a
/// wrapped value.
pub fn ms_of_secs(secs: i64) -> Option<i64> {
    secs.checked_mul(1_000)
}

/// The length rule, overflow-free: either length unknown is `ByTitleAlone`; an impossible length, or one past the slack, `No`.
pub fn fit(track_ms: Option<i64>, file_ms: Option<i64>) -> Fit {
    let (Some(track), Some(file)) = (track_ms, file_ms) else {
        return Fit::ByTitleAlone;
    };
    if !possible(track) || !possible(file) {
        return Fit::No;
    }
    let apart = track.abs_diff(file);
    if apart <= LENGTH_SLACK_MS.unsigned_abs() { Fit::Within(apart as i64) } else { Fit::No }
}

/// The tiers' costs when a solve can make `n` pairs at most: a pair by title alone (n × 2 000 + 1) and a forbidden pair
/// (n times that + 1). None where `n` forbidden pairs would pass what `i64` holds.
pub fn tiers(n: usize) -> Option<(i64, i64)> {
    let n = i64::try_from(n).ok()?;
    let by_title = n.checked_mul(LENGTH_SLACK_MS)?.checked_add(1)?;
    let forbidden = n.checked_mul(by_title)?.checked_add(1)?;
    n.checked_mul(forbidden)?;
    Some((by_title, forbidden))
}

/// One pair's cost, in its tier. A `Within` outside the slack is no length the rule gave: forbidden.
fn cost(fit: Fit, by_title: i64, forbidden: i64) -> i64 {
    match fit {
        Fit::Within(apart) if (0..=LENGTH_SLACK_MS).contains(&apart) => apart,
        Fit::ByTitleAlone => by_title,
        Fit::Within(_) | Fit::No => forbidden,
    }
}

/// Whether a verdict lets a pair stand in the answer.
fn kept(fit: Fit) -> bool {
    match fit {
        Fit::Within(apart) => (0..=LENGTH_SLACK_MS).contains(&apart),
        Fit::ByTitleAlone => true,
        Fit::No => false,
    }
}

/// Whether a file stands at its track's place: both numbers known and the same, on one disc (none is disc 1).
fn at_its_place(track: &Side, file: &Side) -> bool {
    track.number.is_some() && track.number == file.number && track.disc.unwrap_or(1) == file.disc.unwrap_or(1)
}

/// The place only breaks ties: every cost is taken n + 1 times and a file away from its track's place adds 1, so all the
/// places together weigh less than one millisecond. Where that would pass `i64`, 1: ties then go to the keys alone.
fn place_scale(n: usize, forbidden: i64) -> i64 {
    let Some(scale) = n.checked_add(1).and_then(|s| i64::try_from(s).ok()) else {
        return 1;
    };
    let Ok(n) = i64::try_from(n) else {
        return 1;
    };
    let most = forbidden.checked_mul(scale).and_then(|w| w.checked_add(1)).and_then(|w| w.checked_mul(n));
    if most.is_some() { scale } else { 1 }
}

/// The best one-to-one pairing of `tracks` with `files` (PAIR01): the most pairs; then the fewest by title alone; then the
/// least total difference; ties by place, then by the files' keys. `fits(t, f)` is the caller's verdict for track `t` and
/// file `f`: its title rule with [`fit`]. Gives (track, file) pairs, indices into the two lists as given, sorted by track; a
/// forbidden pair is never among them. "Complete" is the caller's check: as many pairs as tracks.
pub fn assign(tracks: &[Side], files: &[Side], fits: impl Fn(usize, usize) -> Fit) -> Vec<(usize, usize)> {
    let n = tracks.len().min(files.len());
    if n == 0 {
        return Vec::new();
    }
    let Some((by_title, forbidden)) = tiers(n) else {
        return Vec::new();
    };
    let scale = place_scale(n, forbidden);
    // The files in one fixed order (T1): by their keys' bytes, never by how they were listed.
    let mut order: Vec<usize> = (0..files.len()).collect();
    order.sort_by(|&x, &y| {
        let (a, b) = (&files[x], &files[y]);
        (a.key, a.disc, a.number, a.length_ms).cmp(&(b.key, b.disc, b.number, b.length_ms))
    });
    // The solver wants no more rows than columns (it panics otherwise): the shorter side is the rows.
    let tracks_lead = tracks.len() <= files.len();
    let (rows, columns) = if tracks_lead { (tracks.len(), files.len()) } else { (files.len(), tracks.len()) };
    let pair_of = |row: usize, column: usize| if tracks_lead { (row, order[column]) } else { (column, order[row]) };
    let Some(cells) = rows.checked_mul(columns) else {
        return Vec::new();
    };
    let mut weights = Vec::with_capacity(cells);
    for row in 0..rows {
        for column in 0..columns {
            let (t, f) = pair_of(row, column);
            let away = if scale > 1 && !at_its_place(&tracks[t], &files[f]) { 1 } else { 0 };
            weights.push(cost(fits(t, f), by_title, forbidden) * scale + away);
        }
    }
    let Ok(matrix) = Matrix::from_vec(rows, columns, weights) else {
        return Vec::new();
    };
    let (_, chosen) = kuhn_munkres_min(&matrix);
    let mut pairs: Vec<(usize, usize)> =
        chosen.into_iter().enumerate().map(|(row, column)| pair_of(row, column)).filter(|&(t, f)| kept(fits(t, f))).collect();
    pairs.sort_unstable();
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Rng;

    // --- Fixtures (SCENARIOS-PAIR01): a list's tracks in ms; a peer's files as Soulseek's caller reads them ---

    /// A track of a list: its title, place and length (ms).
    #[derive(Debug, Clone, Copy)]
    struct T {
        title: &'static str,
        disc: Option<u32>,
        number: Option<u32>,
        ms: Option<i64>,
    }

    fn track(number: u32, title: &'static str, ms: i64) -> T {
        T { title, disc: None, number: Some(number), ms: Some(ms) }
    }

    fn untimed(number: u32, title: &'static str) -> T {
        T { ms: None, ..track(number, title, 0) }
    }

    fn on_disc(disc: u32, number: u32, title: &'static str, ms: i64) -> T {
        T { disc: Some(disc), ..track(number, title, ms) }
    }

    /// A file as Soulseek's caller reads it (the shapes' "Soulseek's `fits`"): its disc from a disc folder ("CD2" is 2), its
    /// number from its name's leading digits, its title from the rest, its length from slskd's whole seconds.
    #[derive(Debug, Clone)]
    struct F {
        path: String,
        disc: Option<u32>,
        number: Option<u32>,
        title: String,
        ms: Option<i64>,
    }

    fn file(path: &str, secs: Option<i64>) -> F {
        let mut segments = path.rsplit(['\\', '/']);
        let name = segments.next().unwrap_or(path);
        let disc = segments.next().and_then(disc_of);
        let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
        let digits: String = stem.chars().take_while(char::is_ascii_digit).collect();
        let title = stem[digits.len()..].trim_start_matches([' ', '-', '.', '_']).to_string();
        F { path: path.to_string(), disc, number: digits.parse().ok(), title, ms: secs.and_then(ms_of_secs) }
    }

    /// "CD2", "Disc 2", "Disk 2" is disc 2; any other folder no disc ("CDs and more" too).
    fn disc_of(folder: &str) -> Option<u32> {
        let folder = folder.to_lowercase();
        let n = folder.strip_prefix("cd").or_else(|| folder.strip_prefix("disc")).or_else(|| folder.strip_prefix("disk"))?;
        n.trim().parse().ok()
    }

    fn base(path: &str) -> &str {
        path.rsplit(['\\', '/']).next().unwrap_or(path)
    }

    /// Pairs as (track, file name), owned.
    fn owned(pairs: &[(usize, &str)]) -> Vec<(usize, String)> {
        pairs.iter().map(|&(t, name)| (t, name.to_string())).collect()
    }

    /// The tests' stand-in for identity's one title rule (X3's; the rule itself stays identity's, C4): the bare titles
    /// agree, and no version word in the file's brackets or " - " part that its track's title lacks (OQ-PAIR01-1 (a) and
    /// OQ-PAIR01-2 (a), the owner 2026-11-20).
    const VERSIONS: [&str; 12] =
        ["instrumental", "live", "remix", "mix", "acoustic", "demo", "karaoke", "acapella", "cappella", "unplugged", "reprise", "edit"];

    fn words(s: &str) -> Vec<String> {
        s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_lowercase).collect()
    }

    /// A title's bare words, and the words around them: in brackets, or after " - ".
    fn parts(title: &str) -> (Vec<String>, Vec<String>) {
        let (mut bare, mut around, mut depth) = (String::new(), String::new(), 0u32);
        for c in title.chars() {
            match c {
                '(' | '[' => {
                    depth += 1;
                    around.push(' ');
                }
                ')' | ']' => {
                    depth = depth.saturating_sub(1);
                    around.push(' ');
                }
                _ if depth > 0 => around.push(c),
                _ => bare.push(c),
            }
        }
        if let Some((head, tail)) = bare.clone().split_once(" - ") {
            bare = head.to_string();
            around.push(' ');
            around.push_str(tail);
        }
        (words(&bare), words(&around))
    }

    fn same_title(track: &str, file: &str) -> bool {
        let (bare, _) = parts(track);
        let (file_bare, around) = parts(file);
        let own = words(track);
        let versioned = around.iter().any(|w| VERSIONS.contains(&w.as_str()) && !own.contains(w));
        !versioned && bare == file_bare
    }

    /// Never, in every scenario (PAIR01's set 1): a file paired to two tracks or a track to two files; an index outside its
    /// list; pairs out of the tracks' order.
    fn one_for_one(pairs: &[(usize, usize)], tracks: usize, files: usize) {
        let (mut track_taken, mut file_taken) = (vec![false; tracks], vec![false; files]);
        for &(t, f) in pairs {
            assert!(t < tracks && f < files, "inside the two lists: {pairs:?}");
            assert!(!track_taken[t] && !file_taken[f], "one for one: {pairs:?}");
            track_taken[t] = true;
            file_taken[f] = true;
        }
        assert!(pairs.windows(2).all(|w| w[0].0 < w[1].0), "sorted by track: {pairs:?}");
    }

    /// `assign` with Soulseek's verdict: the title rule, then the length rule.
    fn pair(tracks: &[T], files: &[F]) -> Vec<(usize, usize)> {
        let t: Vec<Side> = tracks.iter().map(|x| Side { disc: x.disc, number: x.number, key: x.title, length_ms: x.ms }).collect();
        let f: Vec<Side> = files.iter().map(|x| Side { disc: x.disc, number: x.number, key: &x.path, length_ms: x.ms }).collect();
        let pairs =
            assign(&t, &f, |i, j| if same_title(tracks[i].title, &files[j].title) { fit(tracks[i].ms, files[j].ms) } else { Fit::No });
        one_for_one(&pairs, tracks.len(), files.len());
        pairs
    }

    /// Each pair as (track, the file's name): what a pairing is, whatever the files' order.
    fn named(tracks: &[T], files: &[F]) -> Vec<(usize, String)> {
        pair(tracks, files).into_iter().map(|(t, f)| (t, base(&files[f].path).to_string())).collect()
    }

    /// The same, by the file's whole path (two discs' files share a name).
    fn by_path(tracks: &[T], files: &[F]) -> Vec<(usize, String)> {
        pair(tracks, files).into_iter().map(|(t, f)| (t, files[f].path.clone())).collect()
    }

    fn reversed(files: &[F]) -> Vec<F> {
        files.iter().rev().cloned().collect()
    }

    /// Every order of `n` places.
    fn every_order(n: usize) -> Vec<Vec<usize>> {
        if n == 0 {
            return vec![Vec::new()];
        }
        let mut out = Vec::new();
        for rest in every_order(n - 1) {
            for at in 0..=rest.len() {
                let mut order = rest.clone();
                order.insert(at, n - 1);
                out.push(order);
            }
        }
        out
    }

    fn quiet_hours(a: i64, b: i64) -> Vec<T> {
        vec![track(1, "Untitled", a), track(2, "Untitled", b)]
    }

    fn sea_wall() -> Vec<T> {
        vec![track(1, "Pier", 250_000), track(2, "Landing", 240_000), track(3, "Low Water", 260_000)]
    }

    fn long_water(second_intro: i64) -> Vec<T> {
        vec![on_disc(1, 1, "Intro", 60_000), on_disc(1, 2, "Harbour", 240_000), on_disc(2, 1, "Intro", second_intro), on_disc(2, 2, "Tide", 300_000)]
    }

    fn q(name: &str, secs: Option<i64>) -> F {
        file(&format!(r"q\Kite Theory\Quiet Hours\{name}"), secs)
    }

    fn a(name: &str, secs: Option<i64>) -> F {
        file(&format!(r"a\Glass Harbour\Sea Wall\{name}"), secs)
    }

    // --- 1. The pairing itself ---

    /// PAIR01.1 (red first: through `folders_of`, the caller, `a_crossing_folder_is_found`). Today's greedy (app/soulseek.rs
    /// :517-536 at e70e392f7) gives track 1 the first listed of two at 1 000 and leaves track 2 3 000 off, so no folder; the
    /// twin likewise at 4 000 off.
    #[test]
    fn the_crossing_case_pairs_every_track() {
        let crossing = quiet_hours(181_000, 179_000);
        let files = vec![q("01 Untitled.flac", Some(180)), q("02 Untitled.flac", Some(182))];
        let crossed = owned(&[(0, "02 Untitled.flac"), (1, "01 Untitled.flac")]);
        assert_eq!(named(&crossing, &files), crossed, "the one pairing that pairs both, 1 000 off each");
        assert_eq!(named(&crossing, &reversed(&files)), crossed, "the same with the files listed the other way round");
        // The twin, with no tie: 2 000 (the slack's edge) and 1 000.
        let twin = quiet_hours(180_000, 178_000);
        let files = vec![q("01 Untitled.flac", Some(179)), q("02 Untitled.flac", Some(182))];
        assert_eq!(named(&twin, &files), crossed);
        assert_eq!(named(&twin, &reversed(&files)), crossed);
        // Never: the place ("01" for track 1) taking a file past the slack.
        assert_eq!(fit(Some(178_000), ms_of_secs(182)), Fit::No);
    }

    /// PAIR01.2 (guard: green today through the pin
    /// `a_folder_is_a_candidate_only_when_it_holds_every_track_by_title_and_length`).
    #[test]
    fn exact_tracks_pair_straight() {
        let files = vec![a("03 Low Water.flac", Some(260)), a("01 Pier.flac", Some(250)), a("02 Landing.flac", Some(240))];
        assert_eq!(named(&sea_wall(), &files), owned(&[(0, "01 Pier.flac"), (1, "02 Landing.flac"), (2, "03 Low Water.flac")]));
        // A MUSIC copy, tagged 1 "Pier" 250 300 ms, 2 "Landing" 239 800, 3 "Low Water" 260 000.
        let copy = [(1, "Pier", 250_300), (2, "Landing", 239_800), (3, "Low Water", 260_000)];
        let copy: Vec<F> = copy.iter().map(|&(n, t, ms)| F { ms: Some(ms), ..file(&format!("Sea Wall/{n:02} {t}.flac"), None) }).collect();
        assert_eq!(pair(&sea_wall(), &copy), [(0, 0), (1, 1), (2, 2)], "paired 3");
    }

    /// PAIR01.3 (green for Soulseek today; (b)'s copy reads whole by count today, 2 >= 2, against `holding_of`: HD2's row).
    #[test]
    fn a_forbidden_pair_is_never_kept() {
        let wall = sea_wall();
        let two = &wall[..2];
        // (a) 3 000 off: no pair, though a square solve places the one row there.
        assert!(pair(two, &[a("01 Pier.flac", Some(253))]).is_empty());
        // (b) another title at Landing's length, at another place: Pier alone.
        let b = [a("01 Pier.flac", Some(250)), a("03 Breakwater.flac", Some(240))];
        assert_eq!(pair(two, &b), [(0, 0)], "never Landing with Breakwater");
        // (c) another title and another length.
        assert!(pair(two, &[a("02 Breakwater.flac", Some(250))]).is_empty());
    }

    /// PAIR01.4 (a) is red first through `folders_of` in both listing orders (track 1 takes "01" at 0, track 2 is left 4 000
    /// off); (b) is a guard (1.3's `certain_is_todays_judging`, SK3.3 (d)).
    #[test]
    fn the_most_pairs_then_the_least_difference() {
        let files = vec![q("01 Untitled.flac", Some(182)), q("02 Untitled.flac", Some(184))];
        let both = owned(&[(0, "02 Untitled.flac"), (1, "01 Untitled.flac")]);
        assert_eq!(named(&quiet_hours(182_000, 180_000), &files), both, "two pairs at 4 000 before one at 0");
        assert_eq!(named(&quiet_hours(182_000, 180_000), &reversed(&files)), both);
        // The copy of those two files, tagged 182 000 and 184 000: paired 2.
        assert_eq!(pair(&quiet_hours(182_000, 180_000), &files).len(), 2);
        // (b) the straight pairs (0 and 0), never the crossed (1 000 and 1 000).
        let files = vec![q("01 Untitled.flac", Some(180)), q("02 Untitled.flac", Some(181))];
        let straight = owned(&[(0, "01 Untitled.flac"), (1, "02 Untitled.flac")]);
        assert_eq!(named(&quiet_hours(180_000, 181_000), &files), straight);
        assert_eq!(named(&quiet_hours(180_000, 181_000), &reversed(&files)), straight);
    }

    /// PAIR01.5 (c) is red first through the caller, `a_peers_impossible_length_pairs_nothing`: `secs * 1000`
    /// (soulseek.rs:525) panics in a test build for i64::MAX and 2^61 + 250, and a release build wraps the second to 250 000.
    #[test]
    fn costs_are_whole_ms_and_never_overflow() {
        // (a) both within the slack at 1 500: one pair, to the file at Pier's place, in every order.
        let pier = [track(1, "Pier", 251_500)];
        let files = vec![a("01 Pier.flac", Some(250)), a("Pier.flac", Some(253))];
        assert_eq!(named(&pier, &files), owned(&[(0, "01 Pier.flac")]));
        assert_eq!(named(&pier, &reversed(&files)), owned(&[(0, "01 Pier.flac")]));
        assert_eq!(fit(Some(251_500), Some(250_000)), Fit::Within(1_500), "whole ms, an i64");
        // (b) the tiers for n = 300, and the solver's total at most n × F, inside i64.
        let (by_title, forbidden) = tiers(300).unwrap();
        assert_eq!((by_title, forbidden), (600_001, 180_000_301));
        assert_eq!(300 * forbidden, 54_000_090_300);
        assert!(tiers(usize::MAX).is_none(), "a size past what i64 weighs is refused, never wrapped");
        let list: Vec<T> = (1..=300).map(|n| track(n, "Untitled", 3_600_000)).collect();
        let files_of = |secs: Option<i64>| -> Vec<F> { (1..=300).map(|n| q(&format!("{n:03} Untitled.flac"), secs)).collect() };
        assert!(pair(&list, &files_of(Some(60))).is_empty(), "every pair forbidden: none");
        let timed = pair(&list, &files_of(Some(3_600)));
        assert_eq!(timed, (0..300).map(|n| (n, n)).collect::<Vec<_>>(), "every pair 0: 300, each at its place");
        assert_eq!(pair(&list, &files_of(None)).len(), 300, "every pair by title alone: 300");
        // (c) a peer's impossible length: no pair by length; read as unknown where it can't be held, so by title alone at most.
        let wraps = 2_305_843_009_213_694_202_i64;
        assert_eq!(wraps.wrapping_mul(1_000), 250_000, "the case: 2^61 + 250 s wraps to exactly Pier's length");
        for secs in [-5, 0, i64::MAX, wraps] {
            let got = fit(Some(250_000), ms_of_secs(secs));
            assert!(!matches!(got, Fit::Within(_)), "{secs} s is never within the slack: {got:?}");
            let peer = [F { ms: ms_of_secs(secs), ..a("01 Pier.flac", None) }];
            let pairs = pair(&[track(1, "Pier", 250_000)], &peer);
            assert!(pairs.is_empty() || fit(Some(250_000), peer[0].ms) == Fit::ByTitleAlone, "{secs}: {pairs:?}");
        }
        assert_eq!(ms_of_secs(i64::MAX), None);
        assert_eq!(ms_of_secs(wraps), None, "a checked multiply, never a wrap");
        // (d) a list track at i64::MAX or i64::MIN: no pair by length, no overflow.
        for ms in [i64::MAX, i64::MIN] {
            assert_eq!(fit(Some(ms), Some(250_000)), Fit::No);
            assert_eq!(fit(Some(ms), Some(ms)), Fit::No, "a length no file can have is never within the slack");
            assert!(pair(&[T { ms: Some(ms), ..track(1, "Pier", 0) }], &[a("01 Pier.flac", Some(250))]).is_empty());
        }
    }

    /// PAIR01.5 (b2): a guard (green today: greedy gives each track its timed file first) against a tier set too low.
    #[test]
    fn fewer_pairs_by_title_alone_win() {
        let list = vec![track(1, "Untitled", 180_000), track(2, "Untitled", 182_000), track(3, "Untitled", 184_000)];
        let files = vec![q("01 Untitled.flac", None), q("02 Untitled.flac", Some(182)), q("03 Untitled.flac", Some(184)), q("04 Untitled.flac", Some(186))];
        let timed = owned(&[(0, "02 Untitled.flac"), (1, "03 Untitled.flac"), (2, "04 Untitled.flac")]);
        // 6 000, the most three timed pairs can cost, beats A = 6 001 by title alone with 0 and 0; "01" is left over.
        assert_eq!(tiers(3), Some((6_001, 18_004)));
        assert_eq!(named(&list, &files), timed);
        assert_eq!(named(&list, &reversed(&files)), timed);
        // A tier one step above the slack would take the untimed file: 2 001 + 0 + 0 < 6 000.
        assert!(LENGTH_SLACK_MS + 1 < 3 * LENGTH_SLACK_MS);
    }

    /// PAIR01.6 (a guard: a naive `assign` panics on (b), more rows than columns).
    #[test]
    fn any_two_sizes_pair_without_a_panic() {
        // (a) five files: the three pair, never "05 Pier (Live)"; two left over.
        let five = vec![
            a("01 Pier.flac", Some(250)),
            a("02 Landing.flac", Some(240)),
            a("03 Low Water.flac", Some(260)),
            a("04 Breakwater.flac", Some(250)),
            a("05 Pier (Live).flac", Some(250)),
        ];
        assert_eq!(pair(&sea_wall(), &five), [(0, 0), (1, 1), (2, 2)]);
        // (b) two files for three tracks: 2 pairs, the files as rows.
        assert_eq!(pair(&sea_wall(), &five[..2]), [(0, 0), (1, 1)]);
        // (c) an empty folder; an empty list.
        assert!(pair(&sea_wall(), &[]).is_empty());
        assert!(pair(&[], &five).is_empty());
        // Every two sizes up to five, 0 among them.
        for tracks in 0..=5 {
            for files in 0..=5 {
                let list: Vec<T> = (1..=tracks as u32).map(|n| track(n, "Untitled", 180_000)).collect();
                let folder: Vec<F> = (1..=files).map(|n| q(&format!("{n:02} Untitled.flac"), Some(180))).collect();
                assert_eq!(pair(&list, &folder).len(), tracks.min(files), "{tracks} tracks, {files} files");
            }
        }
    }

    /// PAIR01.7 (red first through `folders_of`: the crossing case is missed or found by listing order; PAIR01.14 (b) and
    /// PAIR01.15 (a) cross by listing order).
    #[test]
    fn the_pairing_is_the_same_in_any_file_order() {
        let cases: Vec<(Vec<T>, Vec<F>)> = vec![
            (quiet_hours(181_000, 179_000), vec![q("01 Untitled.flac", Some(180)), q("02 Untitled.flac", Some(182))]),
            (sea_wall(), vec![a("03 Low Water.flac", Some(260)), a("01 Pier.flac", Some(250)), a("02 Landing.flac", Some(240))]),
            (vec![track(1, "Pier", 251_500)], vec![a("01 Pier.flac", Some(250)), a("Pier.flac", Some(253))]),
            (quiet_hours(180_000, 180_000), vec![q("02 Untitled.flac", Some(180)), q("01 Untitled.flac", Some(180))]),
            (
                long_water(60_000),
                vec![
                    file(r"m\Glass Harbour\Long Water\CD2\01 Intro.flac", Some(60)),
                    file(r"m\Glass Harbour\Long Water\CD2\02 Tide.flac", Some(300)),
                    file(r"m\Glass Harbour\Long Water\CD1\01 Intro.flac", Some(60)),
                    file(r"m\Glass Harbour\Long Water\CD1\02 Harbour.flac", Some(240)),
                ],
            ),
            (
                sea_wall(),
                vec![
                    a("05 Pier (Live).flac", Some(250)),
                    a("04 Breakwater.flac", Some(250)),
                    a("01 Pier.flac", Some(250)),
                    a("02 Landing.flac", Some(240)),
                    a("03 Low Water.flac", Some(260)),
                ],
            ),
        ];
        for (list, files) in &cases {
            let first = by_path(list, files);
            assert!(!first.is_empty());
            for order in every_order(files.len()) {
                let shuffled: Vec<F> = order.iter().map(|&i| files[i].clone()).collect();
                assert_eq!(by_path(list, &shuffled), first, "{order:?}");
            }
        }
        // Beyond five files: 50 seeded shuffles of twelve, ten of them one title and one length.
        let list: Vec<T> = (1..=10).map(|n| track(n, "Untitled", 200_000)).collect();
        let mut files: Vec<F> = (1..=12).map(|n| q(&format!("{n:02} Untitled.flac"), Some(200))).collect();
        let first = by_path(&list, &files);
        assert_eq!(first.len(), 10);
        let mut rng = Rng::seeded(7);
        for _ in 0..50 {
            rng.shuffle(&mut files);
            assert_eq!(by_path(&list, &files), first);
        }
    }

    // --- 2. What may pair: titles, lengths, places ---

    /// PAIR01.9 (green for Soulseek today, which reads no place; the holding's (b) and (c) are red against `holding_of`'s
    /// count, under OQ-VR-7 (b): both name and length must agree, the owner 2026-11-20).
    #[test]
    fn the_place_never_outweighs_title_and_length() {
        // (a) numbered in another order: title and length decide.
        let files = vec![a("01 Low Water.flac", Some(260)), a("02 Pier.flac", Some(250)), a("03 Landing.flac", Some(240))];
        assert_eq!(named(&sea_wall(), &files), owned(&[(0, "02 Pier.flac"), (1, "03 Landing.flac"), (2, "01 Low Water.flac")]));
        // (b) place and length agree, the title does not: never paired.
        let files = vec![a("01 Pier.flac", Some(250)), a("02 Breakwater.flac", Some(240)), a("03 Low Water.flac", Some(260))];
        assert_eq!(pair(&sea_wall(), &files), [(0, 0), (2, 2)], "Landing unpaired: paired 2, Partly 2 of 3");
        // (c) place and title agree, the length does not.
        let files = vec![a("01 Pier.flac", Some(250)), a("02 Landing.flac", Some(240)), a("03 Low Water.flac", Some(120))];
        assert_eq!(pair(&sea_wall(), &files), [(0, 0), (1, 1)], "Low Water unpaired: paired 2");
    }

    /// PAIR01.11 (green for Soulseek today, `<=` at soulseek.rs:525; the holding has no pairing).
    #[test]
    fn the_slack_is_two_seconds_inclusive() {
        assert_eq!(LENGTH_SLACK_MS, 2_000);
        let soulseek = |list_ms: i64, secs: i64| fit(Some(list_ms), ms_of_secs(secs));
        assert_eq!(soulseek(250_000, 252), Fit::Within(2_000), "(a)");
        assert_eq!(soulseek(250_000, 248), Fit::Within(2_000), "(b)");
        assert_eq!(soulseek(249_999, 252), Fit::No, "(c) 2 001 off");
        assert_eq!(soulseek(252_001, 250), Fit::No, "(d) 2 001 off");
        assert_eq!(soulseek(262_500, 260), Fit::No, "(e) 2 500 off");
        assert_eq!(fit(Some(250_000), Some(252_000)), Fit::Within(2_000), "(f) a copy's track");
        assert_eq!(fit(Some(250_000), Some(252_001)), Fit::No, "(f) one more");
        assert_eq!(ms_of_secs(252), Some(252_000), "whole seconds as given, × 1 000");
        // Through `assign`: a track at another place (5), so the place decides nothing.
        let pier = [track(5, "Pier", 250_000)];
        assert_eq!(pair(&pier, &[a("01 Pier.flac", Some(252))]), [(0, 0)]);
        assert!(pair(&[track(5, "Pier", 249_999)], &[a("01 Pier.flac", Some(252))]).is_empty());
    }

    /// PAIR01.12 (green today on (a)-(c): `_ => true` and the i64::MAX key, soulseek.rs:526, :531; (c) guards against a tier by
    /// title alone set one step above the slack). (d) waits for the review's word on OQ-PAIR01-3: `fit` reads a copy's file of
    /// no length as by title alone, the shapes' reading; which the holding takes is the store's (C3).
    #[test]
    fn a_pair_by_title_alone_is_the_last_resort() {
        // (a) Pier's file has no length: it pairs by its title alone.
        let files = vec![a("01 Pier.flac", None), a("02 Landing.flac", Some(240)), a("03 Low Water.flac", Some(260))];
        assert_eq!(pair(&sea_wall(), &files), [(0, 0), (1, 1), (2, 2)]);
        assert_eq!(fit(Some(250_000), None), Fit::ByTitleAlone, "never read as 0, which would forbid every pair");
        // (b) a timed file of the same title: it wins, the untimed one is left over.
        let mut more = files.clone();
        more.push(a("Pier.flac", Some(251)));
        assert_eq!(named(&sea_wall(), &more)[0], (0, "Pier.flac".to_string()));
        assert_eq!(pair(&sea_wall(), &more).len(), 3, "extra 1");
        // (c) every pair timed where such a pairing exists, though by title alone sums less.
        let files = vec![q("01 Untitled.flac", None), q("02 Untitled.flac", Some(182)), q("03 Untitled.flac", Some(184))];
        assert_eq!(named(&quiet_hours(180_000, 182_000), &files), owned(&[(0, "02 Untitled.flac"), (1, "03 Untitled.flac")]));
        // (e) a list track of no length pairs by its title alone.
        let list = vec![track(1, "Pier", 250_000), untimed(2, "Landing"), track(3, "Low Water", 260_000)];
        let files = vec![a("01 Pier.flac", Some(250)), a("02 Landing.flac", Some(240)), a("03 Low Water.flac", Some(260))];
        assert_eq!(pair(&list, &files), [(0, 0), (1, 1), (2, 2)]);
        assert_eq!(fit(None, Some(240_000)), Fit::ByTitleAlone);
    }

    /// PAIR01.13 ((b) is red first through `folders_of`: every key is i64::MAX, so track 1 takes the first listed, "02").
    /// Which list reaches `assign` is not its to say (OQ-VR-8 (a), OQ-VR-7 (b), the owner 2026-11-20).
    #[test]
    fn a_list_without_lengths_pairs_by_titles() {
        let discogs = vec![untimed(1, "Pier"), untimed(2, "Landing"), untimed(3, "Low Water")];
        // (a) every pair by title alone; a version file never, without lengths too.
        let g = vec![a("01 Pier.flac", Some(250)), a("02 Landing.flac", Some(240)), a("03 Low Water.flac", Some(260))];
        assert_eq!(pair(&discogs, &g), [(0, 0), (1, 1), (2, 2)]);
        let mut h = g.clone();
        h.push(a("04 Breakwater.flac", Some(250)));
        assert_eq!(pair(&discogs, &h).len(), 3, "the three pair; Breakwater left over");
        let i = vec![a("01 Pier (Live).flac", Some(250)), a("02 Landing.flac", Some(240)), a("03 Low Water.flac", Some(260))];
        assert_eq!(pair(&discogs, &i), [(1, 1), (2, 2)], "Pier unpaired by its version word");
        // (b) two "Untitled" with no lengths: the place, in either order.
        let list = vec![untimed(1, "Untitled"), untimed(2, "Untitled")];
        let files = vec![q("02 Untitled.flac", Some(300)), q("01 Untitled.flac", Some(180))];
        let placed = owned(&[(0, "01 Untitled.flac"), (1, "02 Untitled.flac")]);
        assert_eq!(named(&list, &files), placed);
        assert_eq!(named(&list, &reversed(&files)), placed);
        // (c) a MUSIC copy of the three against this list: paired 3 by titles. Never a length made up for a list without one.
        assert_eq!(pair(&discogs, &g).len(), 3);
        assert!(discogs.iter().all(|t| t.ms.is_none()));
    }

    /// PAIR01.14 ((b) is red first through `folders_of`: track 1 takes the first listed of two at 0, "02 Untitled", and the
    /// gate would hear rec-2 for rec-1).
    #[test]
    fn equal_lengths_part_by_title_then_place() {
        // (a) two titles, one length: the titles part them.
        let list = vec![track(1, "Intro", 60_000), track(2, "Outro", 60_000)];
        let files = vec![q("01 Outro.flac", Some(60)), q("02 Intro.flac", Some(60))];
        assert_eq!(named(&list, &files), owned(&[(0, "02 Intro.flac"), (1, "01 Outro.flac")]));
        // (b) one title, one length: the place breaks the tie, in both orders.
        let files = vec![q("02 Untitled.flac", Some(180)), q("01 Untitled.flac", Some(180))];
        let placed = owned(&[(0, "01 Untitled.flac"), (1, "02 Untitled.flac")]);
        assert_eq!(named(&quiet_hours(180_000, 180_000), &files), placed);
        assert_eq!(named(&quiet_hours(180_000, 180_000), &reversed(&files)), placed);
        // (c) no numbers: one pairing, the same in both orders (by the files' paths).
        let files = vec![q("Untitled.flac", Some(180)), q("Untitled (2).flac", Some(180))];
        let one = named(&quiet_hours(180_000, 180_000), &files);
        assert_eq!(one.len(), 2);
        assert_eq!(named(&quiet_hours(180_000, 180_000), &reversed(&files)), one);
    }

    /// PAIR01.15 ((a) is red first through `folders_of`: disc 1's Intro takes the first listed at 0, `CD2\01 Intro`, and the
    /// files cross).
    #[test]
    fn two_discs_intros_pair_by_disc() {
        let m = |name: &str, secs: i64| file(&format!(r"m\Glass Harbour\Long Water\{name}"), Some(secs));
        let listed = vec![m(r"CD2\01 Intro.flac", 60), m(r"CD2\02 Tide.flac", 300), m(r"CD1\01 Intro.flac", 60), m(r"CD1\02 Harbour.flac", 240)];
        let expected: Vec<(usize, String)> = [r"CD1\01 Intro.flac", r"CD1\02 Harbour.flac", r"CD2\01 Intro.flac", r"CD2\02 Tide.flac"]
            .iter()
            .enumerate()
            .map(|(t, name)| (t, format!(r"m\Glass Harbour\Long Water\{name}")))
            .collect();
        // (a) the disc folders and the numbers: each Intro its own disc's, in any order.
        assert_eq!(by_path(&long_water(60_000), &listed), expected);
        assert_eq!(by_path(&long_water(60_000), &reversed(&listed)), expected);
        // (b) the same files under "Disc 1" and "Disc 2".
        let n = |name: &str, secs: i64| file(&format!(r"n\Long Water\{name}"), Some(secs));
        let discs = vec![n(r"Disc 2\01 Intro.flac", 60), n(r"Disc 2\02 Tide.flac", 300), n(r"Disc 1\01 Intro.flac", 60), n(r"Disc 1\02 Harbour.flac", 240)];
        assert_eq!(pair(&long_water(60_000), &discs), [(0, 2), (1, 3), (2, 0), (3, 1)]);
        // (d) a copy's tags, discnumber 1 and 2; then disc 1's files with no discnumber: disc 1, paired 4 both times.
        let tagged = |disc: Option<u32>, number: u32, title: &str, ms: i64| F {
            path: format!("Long Water/{}-{number:02} {title}.flac", disc.unwrap_or(1)),
            disc,
            number: Some(number),
            title: title.to_string(),
            ms: Some(ms),
        };
        let copy = vec![tagged(Some(1), 1, "Intro", 60_000), tagged(Some(1), 2, "Harbour", 240_000), tagged(Some(2), 1, "Intro", 60_000), tagged(Some(2), 2, "Tide", 300_000)];
        assert_eq!(pair(&long_water(60_000), &copy), [(0, 0), (1, 1), (2, 2), (3, 3)]);
        let untagged = vec![tagged(None, 1, "Intro", 60_000), tagged(None, 2, "Harbour", 240_000), tagged(Some(2), 1, "Intro", 60_000), tagged(Some(2), 2, "Tide", 300_000)];
        assert_eq!(pair(&long_water(60_000), &untagged), [(0, 0), (1, 1), (2, 2), (3, 3)]);
        // (e) the lengths decide over the disc folders.
        let e = vec![m(r"CD2\01 Intro.flac", 60), m(r"CD2\02 Tide.flac", 300), m(r"CD1\01 Intro.flac", 75), m(r"CD1\02 Harbour.flac", 240)];
        assert_eq!(pair(&long_water(75_000), &e), [(0, 0), (1, 3), (2, 2), (3, 1)]);
        // A folder named "CDs and more" is no disc (the pin).
        assert_eq!(disc_of("CDs and more"), None);
    }

    /// PAIR01.16 (the holding counts today: (b) reads whole, red under OQ-VR-7 (b); (e) reads 3 of 4).
    #[test]
    fn a_hidden_or_bonus_track_pairs_once_or_not() {
        let three = vec![a("01 Pier.flac", Some(250)), a("02 Landing.flac", Some(240)), a("03 Low Water.flac", Some(260))];
        // (a) a bonus file past the list; (c) a hidden track before track 1: the three pair, the extra is left over.
        let mut bonus = three.clone();
        bonus.push(a("04 Breakwater (Bonus Track).flac", Some(200)));
        assert_eq!(pair(&sea_wall(), &bonus), [(0, 0), (1, 1), (2, 2)]);
        let mut hidden = vec![a("00 Hidden.flac", Some(95))];
        hidden.extend(three.iter().cloned());
        assert_eq!(pair(&sea_wall(), &hidden), [(0, 1), (1, 2), (2, 3)]);
        // (b) a hidden track inside the last file: 452 s off, never paired.
        let long_last = vec![a("01 Pier.flac", Some(250)), a("02 Landing.flac", Some(240)), a("03 Low Water.flac", Some(712))];
        assert_eq!(pair(&sea_wall(), &long_last), [(0, 0), (1, 1)], "Partly 2 of 3");
        // (d) the bonus on a list, not in the copy: 3 of 4; against the three, whole.
        let mut four = sea_wall();
        four.push(track(4, "Breakwater (Bonus Track)", 200_000));
        assert_eq!(pair(&four, &three).len(), 3);
        assert_eq!(pair(&sea_wall(), &three).len(), 3);
        // (e) the list names the hidden track on its own; the copy holds it inside "03 Low Water": one track at most.
        let mut named_hidden = sea_wall();
        named_hidden.push(track(4, "Untitled (Hidden Track)", 380_000));
        assert_eq!(pair(&named_hidden, &long_last), [(0, 0), (1, 1)], "paired 2 of 4");
    }
}
