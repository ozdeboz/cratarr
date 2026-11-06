//! Which new file is a gone file that moved (decision 0011 §4). Only for changes made outside Cratarr: Cratarr's own
//! moves and replacements write the track id themselves. Pure.
//!
//! A pair is made only when it is unambiguous both ways: one new file and one gone file fit, and no other. The gone
//! files of one track are one fit: they are its copies, and any of them gives the new file the same id. Size and tags
//! alone never pair two files, so retagging or a format change made elsewhere gives a new id rather than a wrong one.

use std::collections::BTreeMap;

/// What is compared, already folded (lower case, letters and digits only).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileFacts {
    /// The catalogue folder it is in: no rule reads it, [`match_moves_by_folder`] sorts by it.
    pub folder: i64,
    /// The track a gone file holds: no rule reads it, it says which gone files are one fit. None: it stands alone.
    pub track: Option<i64>,
    /// The file name, without folders.
    pub name: String,
    pub duration_ms: Option<i64>,
    /// MusicBrainz recording id.
    pub recording: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
}

/// How far two lengths may differ and still be the same recording.
const SAME_LENGTH_MS: i64 = 2_000;

pub fn fold(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// The file name of a relative path, folded, without its extension.
pub fn name_of(rel: &str) -> String {
    let file = rel.rsplit('/').next().unwrap_or(rel);
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    fold(stem)
}

fn same_length(a: &FileFacts, b: &FileFacts) -> bool {
    matches!((a.duration_ms, b.duration_ms), (Some(x), Some(y)) if (x - y).abs() <= SAME_LENGTH_MS)
}

fn some_eq(a: &Option<String>, b: &Option<String>) -> bool {
    matches!((a, b), (Some(x), Some(y)) if !x.is_empty() && x == y)
}

type Rule = fn(&FileFacts, &FileFacts) -> bool;

/// Rules in order; the first that pairs a file decides.
const RULES: [Rule; 3] = [
    // The same MusicBrainz recording.
    |n, g| some_eq(&n.recording, &g.recording),
    // Renamed folder, same file name and length (and title, when both have one).
    |n, g| {
        !n.name.is_empty()
            && n.name == g.name
            && same_length(n, g)
            && (n.title.is_none() || g.title.is_none() || n.title == g.title)
    },
    // Renamed file: same title, artist and length.
    |n, g| some_eq(&n.title, &g.title) && some_eq(&n.artist, &g.artist) && same_length(n, g),
];

/// Pairs of (new index, gone index): each new file takes the gone file's track id.
pub fn match_moves(new: &[FileFacts], gone: &[FileFacts]) -> Vec<(usize, usize)> {
    match_within(new, gone, &[None])
}

/// [`match_moves`] with each rule tried within each folder first, then across folders on what is left. Two copies of a
/// track renamed outside Cratarr fit each other's gone rows too, and that ambiguity would give both fresh ids and split
/// the track; within its folder each fits one. A file moved from one folder to another still pairs, in a rule's last
/// round. The folders sit inside the rules: they settle what one rule alone cannot, and a weak fit in a file's own folder
/// must not beat a surer rule's fit in another.
pub fn match_moves_by_folder(new: &[FileFacts], gone: &[FileFacts]) -> Vec<(usize, usize)> {
    let mut rounds: Vec<Option<i64>> = new.iter().map(|f| Some(f.folder)).collect();
    rounds.sort_unstable();
    rounds.dedup();
    rounds.push(None);
    match_within(new, gone, &rounds)
}

/// Every rule in order, and each rule over `rounds` in order: the files of one folder, or None for all of them.
fn match_within(new: &[FileFacts], gone: &[FileFacts], rounds: &[Option<i64>]) -> Vec<(usize, usize)> {
    let mut new_free = vec![true; new.len()];
    let mut gone_free = vec![true; gone.len()];
    let mut pairs = Vec::new();
    for rule in RULES {
        for &folder in rounds {
            let here = |f: &FileFacts| folder.is_none_or(|x| x == f.folder);
            let new_here: Vec<usize> = (0..new.len()).filter(|&n| new_free[n] && here(&new[n])).collect();
            let gone_here: Vec<usize> = (0..gone.len()).filter(|&g| gone_free[g] && here(&gone[g])).collect();
            for (n, g) in pairs_by(rule, new, gone, &new_here, &gone_here) {
                new_free[n] = false;
                gone_free[g] = false;
                pairs.push((n, g));
            }
        }
    }
    pairs.sort();
    pairs
}

/// What a new file can fit under a rule: one gone file, or all the gone files of one track.
#[derive(Default)]
struct Target {
    gone: Vec<usize>,
    /// How many new files fit it, whatever else they fit.
    fitting: usize,
    /// The new files that fit it and nothing else, each with the gone files it fits.
    only: Vec<(usize, Vec<usize>)>,
}

/// One rule's pairs among these free files.
fn pairs_by(rule: Rule, new: &[FileFacts], gone: &[FileFacts], new_here: &[usize], gone_here: &[usize]) -> Vec<(usize, usize)> {
    let target_of = |g: usize| gone[g].track.map_or((None, g), |t| (Some(t), 0));
    let mut targets: BTreeMap<(Option<i64>, usize), Target> = BTreeMap::new();
    for &g in gone_here {
        targets.entry(target_of(g)).or_default().gone.push(g);
    }
    for &n in new_here {
        let fits: Vec<usize> = gone_here.iter().copied().filter(|&g| rule(&new[n], &gone[g])).collect();
        let mut fit: Vec<_> = fits.iter().map(|&g| target_of(g)).collect();
        fit.sort_unstable();
        fit.dedup();
        for t in &fit {
            targets.entry(*t).or_default().fitting += 1;
        }
        if let [t] = fit[..] {
            targets.entry(t).or_default().only.push((n, fits));
        }
    }
    let mut pairs = Vec::new();
    for mut t in targets.into_values() {
        // Unambiguous the other way too: no more new files fit it than it has gone files for them to take.
        if t.fitting > t.gone.len() {
            continue;
        }
        for (n, fits) in t.only {
            // Its own where one is left, else another of the track's, taken by a file that fits both: the id is the same.
            let g = fits.iter().copied().find(|g| t.gone.contains(g)).unwrap_or(t.gone[0]);
            t.gone.retain(|&x| x != g);
            pairs.push((n, g));
        }
    }
    pairs
}

/// A track of a folder as a record's tracklist compares it: folded title and length.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub title: String,
    pub duration_ms: i64,
}

/// Whether two tracklists, in order, are the same tracks (0101: the same record is the same tracks): as many, each the
/// same title, each within [`SAME_LENGTH_MS`]. An empty list is nothing's tracklist.
pub fn same_tracks(a: &[Listed], b: &[Listed]) -> bool {
    !a.is_empty()
        && a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| !x.title.is_empty() && x.title == y.title && (x.duration_ms - y.duration_ms).abs() <= SAME_LENGTH_MS)
}

/// What decides whether two folders are copies of one record ([`same_record`]), folded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordFacts {
    /// MusicBrainz release.
    pub release: Option<String>,
    pub artist: String,
    pub title: String,
    pub tracks: Vec<Listed>,
}

/// Whether two folders are copies of one record (docs/records-and-copies.md): the tracks must match, always; with that,
/// two MusicBrainz releases decide, and without one on either side the album artist and title do. What cannot be told
/// apart stays two (0100).
pub fn same_record(a: &RecordFacts, b: &RecordFacts) -> bool {
    if !same_tracks(&a.tracks, &b.tracks) {
        return false;
    }
    match (&a.release, &b.release) {
        (Some(x), Some(y)) => x == y,
        _ => !a.title.is_empty() && a.title == b.title && a.artist == b.artist,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(release: Option<&str>, artist: &str, title: &str, tracks: &[(&str, i64)]) -> RecordFacts {
        RecordFacts {
            release: release.map(fold),
            artist: fold(artist),
            title: fold(title),
            tracks: tracks.iter().map(|(t, ms)| Listed { title: fold(t), duration_ms: *ms }).collect(),
        }
    }

    #[test]
    fn the_same_record_is_the_same_tracks() {
        let two = [("One", 200_000), ("Two", 180_000)];
        let album = rec(None, "A", "B", &two);
        assert!(same_record(&album, &rec(None, "A", "B", &[("one", 201_500), ("Two!", 180_000)])), "lengths within 2 s, loose titles");
        assert!(!same_record(&album, &rec(None, "A", "B", &[("One", 200_000)])), "a track missing");
        assert!(!same_record(&album, &rec(None, "A", "B", &[("One", 200_000), ("Two", 180_000), ("Bonus", 90_000)])), "a deluxe edition");
        assert!(!same_record(&album, &rec(None, "A", "B", &[("One", 200_000), ("Two", 190_000)])), "another length");
        assert!(!same_record(&album, &rec(None, "A", "C", &two)), "another title");
        assert!(!same_record(&album, &rec(None, "D", "B", &two)), "another artist");
    }

    #[test]
    fn a_release_decides_only_among_the_same_tracks() {
        let two = [("One", 200_000), ("Two", 180_000)];
        let tagged = rec(Some("r1"), "A", "B", &two);
        assert!(same_record(&tagged, &rec(Some("r1"), "A (remaster tag)", "B [2011]", &two)), "the same release, tags apart");
        assert!(!same_record(&tagged, &rec(Some("r2"), "A", "B", &two)), "two releases are two records");
        assert!(!same_record(&tagged, &rec(Some("r1"), "A", "B", &[("One", 200_000)])), "a folder missing a track is no copy");
        assert!(same_record(&tagged, &rec(None, "A", "B", &two)), "one side without a release: artist and title");
        assert!(!same_record(&rec(None, "", "", &[]), &rec(None, "", "", &[])), "nothing is no record");
    }

    fn f(name: &str, ms: i64, recording: Option<&str>, title: Option<&str>, artist: Option<&str>) -> FileFacts {
        FileFacts {
            folder: 0,
            track: None,
            name: name_of(name),
            duration_ms: Some(ms),
            recording: recording.map(fold),
            title: title.map(fold),
            artist: artist.map(fold),
        }
    }

    /// One song, as a file of a folder.
    fn intro(folder: i64, path: &str) -> FileFacts {
        FileFacts { folder, ..f(path, 60_000, None, Some("Intro"), Some("X")) }
    }

    /// That song as a gone file of a track.
    fn gone_copy(track: i64, folder: i64, path: &str) -> FileFacts {
        FileFacts { track: Some(track), ..intro(folder, path) }
    }

    #[test]
    fn names_and_folding() {
        assert_eq!(name_of("Air - Moon Safari (1998)/01 La Femme d'Argent.flac"), "01lafemmedargent");
        assert_eq!(fold("Đ.K. – 4/4"), "đk44");
    }

    #[test]
    fn the_recording_id_pairs_first_whatever_else_changed() {
        let new = vec![f("x/01 Other Name.flac", 300_000, Some("rec-1"), Some("New Title"), None)];
        let gone = vec![f("y/01 Song.mp3", 299_000, Some("rec-1"), Some("Song"), None)];
        assert_eq!(match_moves(&new, &gone), vec![(0, 0)]);
    }

    #[test]
    fn a_renamed_folder_pairs_by_file_name_and_length() {
        let new = vec![f("Album (2021)/01 Song.flac", 200_000, None, Some("Song"), None)];
        let gone = vec![f("Album/01 Song.flac", 200_500, None, Some("Song"), None)];
        assert_eq!(match_moves(&new, &gone), vec![(0, 0)]);
        let longer = vec![f("Album (2021)/01 Song.flac", 260_000, None, Some("Song"), None)];
        assert!(match_moves(&longer, &gone).is_empty(), "a different length is a different recording");
    }

    #[test]
    fn a_renamed_file_pairs_by_title_artist_and_length() {
        let new = vec![f("a/1-01 Track.flac", 180_000, None, Some("Track"), Some("Artist"))];
        let gone = vec![f("a/01 Track.flac", 181_000, None, Some("Track"), Some("Artist"))];
        assert_eq!(match_moves(&new, &gone), vec![(0, 0)]);
    }

    #[test]
    fn ambiguity_is_never_guessed() {
        // Two gone copies of the same song fit one new file: no pair.
        let new = vec![f("c/01 Intro.flac", 60_000, None, Some("Intro"), Some("X"))];
        let gone = vec![f("a/01 Intro.flac", 60_000, None, Some("Intro"), Some("X")), f("b/01 Intro.flac", 60_000, None, Some("Intro"), Some("X"))];
        assert!(match_moves(&new, &gone).is_empty());
        // Two new files fit one gone file: no pair either.
        let two_new = vec![gone[0].clone(), gone[1].clone()];
        assert!(match_moves(&two_new, &gone[..1]).is_empty());
    }

    #[test]
    fn a_rename_pairs_in_its_own_folder() {
        let new = vec![intro(0, "a2/01 Intro.flac"), intro(1, "b2/01 Intro.flac")];
        let gone = vec![intro(1, "b/01 Intro.flac"), intro(0, "a/01 Intro.flac")];
        assert!(match_moves(&new, &gone).is_empty(), "across folders each fits both");
        assert_eq!(match_moves_by_folder(&new, &gone), vec![(0, 1), (1, 0)]);
    }

    #[test]
    fn a_file_moved_between_folders_still_pairs() {
        let new = vec![intro(1, "b/01 Intro.flac")];
        let gone = vec![intro(0, "a/01 Intro.flac")];
        assert_eq!(match_moves_by_folder(&new, &gone), vec![(0, 0)]);
        let other = f("a/02 Other.flac", 90_000, None, Some("Other"), None);
        let (new, gone) = (vec![new[0].clone(), other.clone()], vec![gone[0].clone(), other]);
        assert_eq!(match_moves_by_folder(&new, &gone), vec![(0, 0), (1, 1)], "beside a file its own folder paired");
    }

    #[test]
    fn ambiguity_within_a_folder_is_not_guessed_across_them() {
        let new = vec![intro(0, "c/01 Intro.flac")];
        let gone = vec![intro(0, "a/01 Intro.flac"), intro(0, "b/01 Intro.flac"), intro(1, "d/01 Intro.flac")];
        assert!(match_moves_by_folder(&new, &gone).is_empty());
    }

    #[test]
    fn gone_copies_of_one_track_are_one_fit() {
        let new = vec![intro(0, "a2/01 Intro.flac"), intro(0, "b2/01 Intro.flac")];
        let gone = vec![gone_copy(7, 0, "a/01 Intro.flac"), gone_copy(7, 0, "b/01 Intro.flac")];
        assert_eq!(match_moves_by_folder(&new, &gone), vec![(0, 0), (1, 1)], "both renamed in one folder");
        assert_eq!(match_moves(&new[..1], &gone), vec![(0, 0)], "one back: the other gone file waits");
        let spread = vec![gone[0].clone(), gone_copy(7, 1, "b/01 Intro.flac")];
        assert_eq!(match_moves_by_folder(&new, &spread), vec![(0, 0), (1, 1)], "too few in the folder: across them");
    }

    #[test]
    fn a_track_takes_no_more_new_files_than_it_has_gone_files() {
        let new = vec![intro(0, "a2/01 Intro.flac"), intro(0, "b2/01 Intro.flac"), intro(0, "c/01 Intro.flac")];
        let gone = vec![gone_copy(7, 0, "a/01 Intro.flac"), gone_copy(7, 0, "b/01 Intro.flac")];
        assert!(match_moves_by_folder(&new, &gone).is_empty(), "one of three is no copy of it, and which is not known");
    }

    #[test]
    fn two_tracks_are_two_fits() {
        let new = vec![intro(0, "a2/01 Intro.flac"), intro(0, "b2/01 Intro.flac")];
        let gone = vec![gone_copy(7, 0, "a/01 Intro.flac"), gone_copy(8, 0, "b/01 Intro.flac")];
        assert!(match_moves_by_folder(&new, &gone).is_empty());
        // The second new file fits both tracks and pairs with neither, and still stands in the first one's way.
        let (near, far) = (FileFacts { duration_ms: Some(60_000), ..gone[0].clone() }, FileFacts { duration_ms: Some(62_500), ..gone[1].clone() });
        let new = vec![FileFacts { duration_ms: Some(59_000), ..new[0].clone() }, FileFacts { duration_ms: Some(61_000), ..new[1].clone() }];
        assert!(match_moves_by_folder(&new, &[near, far]).is_empty());
    }

    /// The first new file fits both gone files and takes the second's only one: the track is what it was after.
    #[test]
    fn a_copy_takes_its_track_though_another_took_its_gone_file() {
        let gone = vec![
            FileFacts { duration_ms: Some(60_000), ..gone_copy(7, 0, "a/01 Intro.flac") },
            FileFacts { duration_ms: Some(61_500), ..gone_copy(7, 0, "b/01 Intro.flac") },
        ];
        let new = vec![FileFacts { duration_ms: Some(60_500), ..intro(0, "a2/01 Intro.flac") }, FileFacts { duration_ms: Some(58_500), ..intro(0, "b2/01 Intro.flac") }];
        assert_eq!(match_moves_by_folder(&new, &gone), vec![(0, 0), (1, 1)]);
    }

    /// The folder settles what one rule alone cannot, never a surer rule's answer: the weaker fit is another album's file.
    #[test]
    fn a_recording_in_another_folder_beats_a_weaker_fit_in_the_files_own() {
        let new = vec![FileFacts { folder: 0, ..f("c/01 Intro.flac", 60_000, Some("r1"), Some("Intro"), Some("X")) }];
        let moved = FileFacts { folder: 1, ..f("a/01 Intro.flac", 60_000, Some("r1"), Some("Intro"), Some("X")) };
        let weaker = FileFacts { folder: 0, ..f("b/01 Intro.flac", 61_000, Some("r0"), Some("Intro"), Some("X")) };
        assert_eq!(match_moves_by_folder(&new, &[moved.clone(), weaker.clone()]), vec![(0, 0)]);
        let here = FileFacts { folder: 0, ..moved.clone() };
        assert_eq!(match_moves_by_folder(&new, &[moved, here, weaker]), vec![(0, 1)], "the same rule in two folders: its own");
    }

    #[test]
    fn size_and_tags_alone_never_pair() {
        // Same title and artist but no length: not enough.
        let mut new = f("a/x.flac", 0, None, Some("Song"), Some("A"));
        new.duration_ms = None;
        let gone = vec![f("b/y.mp3", 200_000, None, Some("Song"), Some("A"))];
        assert!(match_moves(&[new], &gone).is_empty());
    }

    #[test]
    fn each_file_is_used_once_across_rules() {
        let new = vec![f("n/01 A.flac", 100_000, Some("r1"), Some("A"), None), f("n/02 B.flac", 100_000, None, Some("B"), None)];
        let gone = vec![f("o/01 A.flac", 100_000, Some("r1"), Some("A"), None), f("o/02 B.flac", 100_000, None, Some("B"), None)];
        assert_eq!(match_moves(&new, &gone), vec![(0, 0), (1, 1)]);
    }
}
