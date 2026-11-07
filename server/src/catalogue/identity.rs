//! Which new file is a gone file that moved (decision 0011 §4). Only for changes made outside Cratarr: Cratarr's own
//! moves and replacements write the track id themselves. Pure.
//!
//! A pair is made only when it is unambiguous both ways: one new file and one gone file fit, and no other. The gone
//! files of one track are one fit: they are its copies, and any of them gives the new file the same id. The surest fit
//! comes first: a `mv` keeps a file's size and modified time, which together with its length say it is that file, whatever it is
//! called now and whatever it says of itself. Short of that, tags alone never pair two files, so retagging or a format
//! change made elsewhere gives a new id rather than a wrong one.

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
    /// Its size and modified time. None for what has no file any more, a retired track: no exact fit there.
    pub size: Option<i64>,
    pub mtime: Option<i64>,
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
const RULES: [Rule; 4] = [
    // The file itself, moved: the same size, modified time and length.
    |n, g| matches!((n.size, g.size), (Some(x), Some(y)) if x == y) && matches!((n.mtime, g.mtime), (Some(x), Some(y)) if x == y) && matches!((n.duration_ms, g.duration_ms), (Some(x), Some(y)) if x == y),
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

/// The words that name a guest in a title, longest first so "feat." is not read as "feat" and a dot.
const GUEST_WORDS: [&str; 4] = ["featuring", "feat.", "feat", "ft."];

/// How long the guest word starting at `at` is, where one starts there as a word of its own ("Defeat" holds none).
fn guest_word(lower: &str, at: usize) -> Option<usize> {
    if lower[..at].chars().next_back().is_some_and(char::is_alphanumeric) {
        return None;
    }
    GUEST_WORDS.iter().find(|w| lower[at..].starts_with(**w) && !lower[at + w.len()..].chars().next().is_some_and(char::is_alphanumeric)).map(|w| w.len())
}

/// A title as its bare name and the guests it names, as written (the owner, 2026-11-06: a guest is recognised, not
/// ignored): "Embrace (feat. Phoebe Killdeer)" is the title "Embrace" with a guest. "feat.", "feat", "ft." and "featuring",
/// in brackets or not: out of brackets the guests run to the next bracket or the end, so "A feat. B (Remix)" stays
/// "A (Remix)". A word that leaves no name ("Feat of Clay") is the title's own.
pub fn title_and_guests(title: &str) -> (String, Vec<String>) {
    let (mut bare, mut guests) = (title.to_string(), Vec::new());
    let mut from = 0;
    loop {
        // Lower case by ASCII alone: every place in it is the same place in `bare`.
        let lower = bare.to_ascii_lowercase();
        let Some((at, len)) = lower.char_indices().filter(|(i, _)| *i >= from).find_map(|(i, _)| guest_word(&lower, i).map(|n| (i, n))) else { break };
        let names = at + len;
        let before = bare[..at].trim_end();
        // In brackets of its own when a bracket opens right before the word.
        let (start, end, past) = match before.chars().next_back().filter(|c| "([{".contains(*c)) {
            Some(open) => {
                let close = match open {
                    '(' => ')',
                    '[' => ']',
                    _ => '}',
                };
                let end = bare[names..].find(close).map_or(bare.len(), |i| names + i);
                (before.len() - 1, end, (end + 1).min(bare.len()))
            }
            None => {
                let end = bare[names..].find(['(', ')', '[', ']', '{', '}']).map_or(bare.len(), |i| names + i);
                (at, end, end)
            }
        };
        let left = format!("{}{}", &bare[..start], &bare[past..]);
        if fold(&left).is_empty() {
            from = names;
            continue;
        }
        guests.extend(guests_in(&bare[names..end]));
        bare = left;
        from = start;
    }
    (bare.trim().to_string(), guests)
}

/// The guests in the words after a guest word: "A, B & C and D".
fn guests_in(names: &str) -> Vec<String> {
    names.replace(" and ", ",").replace(" And ", ",").split([',', '&']).map(str::trim).filter(|n| !n.is_empty()).map(str::to_string).collect()
}

/// A track of a folder as a record's tracklist compares it: its bare name and guests, folded, and its length.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub title: String,
    /// Sorted: one list of guests in two orders is one list.
    pub guests: Vec<String>,
    pub duration_ms: i64,
}

impl Listed {
    /// A track by its title as tagged.
    pub fn of(title: &str, duration_ms: i64) -> Self {
        let (bare, guests) = title_and_guests(title);
        let mut guests: Vec<String> = guests.iter().map(|g| fold(g)).filter(|g| !g.is_empty()).collect();
        guests.sort_unstable();
        guests.dedup();
        Self { title: fold(&bare), guests, duration_ms }
    }

    /// One name: the bare names match, and the guests agree or one side names none. Two different guests are two tracks.
    fn same_name(&self, other: &Self) -> bool {
        !self.title.is_empty() && self.title == other.title && (self.guests == other.guests || self.guests.is_empty() || other.guests.is_empty())
    }

    /// One track: one name, within [`SAME_LENGTH_MS`].
    pub fn same_track(&self, other: &Self) -> bool {
        self.same_name(other) && (self.duration_ms - other.duration_ms).abs() <= SAME_LENGTH_MS
    }
}

/// Whether two tracklists, in order, are the same tracks (0101: the same record is the same tracks): as many, each the
/// same name, each within [`SAME_LENGTH_MS`]. An empty list is nothing's tracklist.
pub fn same_tracks(a: &[Listed], b: &[Listed]) -> bool {
    !a.is_empty() && a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.same_track(y))
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

/// Whether two folders say the very same MusicBrainz release.
pub fn same_release(a: &RecordFacts, b: &RecordFacts) -> bool {
    matches!((&a.release, &b.release), (Some(x), Some(y)) if x == y)
}

/// Whether of two folders of the very same release one is the other with tracks still to come (the owner, 2026-11-07): the one
/// case the tracklists need not match. Every track of the shorter must be one of the longer's, that one alone, or a file would
/// take a track it is not: a track the other does not list makes it something else, and of two tracks alike a file cannot say
/// which it is.
pub fn short_of(a: &RecordFacts, b: &RecordFacts) -> bool {
    let (short, long) = if a.tracks.len() < b.tracks.len() { (&a.tracks, &b.tracks) } else { (&b.tracks, &a.tracks) };
    let at_most_one = |these: &[Listed], those: &[Listed]| these.iter().all(|t| those.iter().filter(|o| t.same_track(o)).count() <= 1);
    same_release(a, b)
        && !short.is_empty()
        && short.len() < long.len()
        && short.iter().all(|t| long.iter().any(|o| t.same_track(o)))
        && at_most_one(short, long)
        && at_most_one(long, short)
}

/// Whether two folders are copies of one record (docs/records-and-copies.md): the tracks must match, always; with that,
/// two MusicBrainz releases decide, and without one on either side the album artist and title do. What cannot be told
/// apart stays two (0100). A folder short of its release is not this: [`short_of`].
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
            tracks: tracks.iter().map(|(t, ms)| Listed::of(t, *ms)).collect(),
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

    /// The owner, 2026-11-07: a folder of the very same release is that record with tracks still to come, whichever came first.
    #[test]
    fn a_folder_of_the_very_same_release_can_be_short_of_it() {
        let whole = rec(Some("r1"), "A", "B", &[("One", 200_000), ("Two", 180_000), ("Three", 150_000)]);
        let (first, gap) = (rec(Some("r1"), "A", "B", &[("Three", 150_000)]), rec(Some("r1"), "A", "B", &[("One", 200_000), ("Three", 150_000)]));
        assert!(short_of(&first, &whole) && short_of(&gap, &whole), "whichever are missing");
        assert!(short_of(&whole, &gap), "either way round");
        assert!(short_of(&rec(Some("r1"), "Tagged Otherwise", "So Is This", &[("One", 200_000)]), &whole), "the release says, not the names");
        assert!(!same_record(&gap, &whole), "`same_record` stays as it is");
        assert!(!short_of(&whole, &whole), "the same tracks are `same_record`'s");
    }

    #[test]
    fn nothing_but_the_very_same_release_with_its_own_tracks_is_short_of_it() {
        let whole = rec(Some("r1"), "A", "B", &[("One", 200_000), ("Two", 180_000), ("Three", 150_000)]);
        assert!(!short_of(&rec(None, "A", "B", &[("One", 200_000)]), &whole), "no release on one side");
        assert!(!short_of(&rec(None, "A", "B", &[("One", 200_000)]), &rec(None, "A", "B", &[("One", 200_000), ("Two", 180_000)])), "nor on either");
        assert!(!short_of(&rec(Some("r2"), "A", "B", &[("One", 200_000)]), &whole), "another release");
        assert!(!short_of(&rec(Some("r1"), "A", "B", &[("Bonus", 200_000)]), &whole), "a track the record does not list");
        assert!(!short_of(&rec(Some("r1"), "A", "B", &[("One", 200_000), ("Bonus", 90_000)]), &whole), "one among its own");
        assert!(!short_of(&rec(Some("r1"), "A", "B", &[("One", 230_000)]), &whole), "a length still decides");
        assert!(!short_of(&rec(Some("r1"), "A", "B", &[]), &whole), "nothing is no part of it");
    }

    /// A take cannot be undone: a file that could be either of two tracks is neither.
    #[test]
    fn of_two_tracks_alike_a_file_cannot_say_which_it_is() {
        let twice = rec(Some("r1"), "A", "B", &[("Interlude", 60_000), ("One", 200_000), ("Interlude", 60_000)]);
        assert!(!short_of(&rec(Some("r1"), "A", "B", &[("Interlude", 60_000)]), &twice));
        assert!(short_of(&rec(Some("r1"), "A", "B", &[("One", 200_000)]), &twice), "a track of its own name is no question");
        let two_files = rec(Some("r1"), "A", "B", &[("One", 200_000), ("One", 200_000)]);
        assert!(!short_of(&two_files, &rec(Some("r1"), "A", "B", &[("One", 200_000), ("Two", 180_000), ("Three", 150_000)])), "two files, one track");
    }

    #[test]
    fn a_title_splits_into_its_name_and_its_guests() {
        let split = |t: &str| title_and_guests(t);
        let guest = |n: &str, g: &[&str]| (n.to_string(), g.iter().map(|x| x.to_string()).collect::<Vec<_>>());
        assert_eq!(split("Embrace (feat. Phoebe Killdeer)"), guest("Embrace", &["Phoebe Killdeer"]));
        assert_eq!(split("Embrace [Feat Phoebe Killdeer]"), guest("Embrace", &["Phoebe Killdeer"]));
        assert_eq!(split("Embrace ft. Phoebe Killdeer"), guest("Embrace", &["Phoebe Killdeer"]));
        assert_eq!(split("Embrace FEATURING A, B & C and D"), guest("Embrace", &["A", "B", "C", "D"]));
        assert_eq!(split("Sloth feat. Mood Tattooed (Remix)"), guest("Sloth (Remix)", &["Mood Tattooed"]));
        assert_eq!(split("Sloth (feat. Mood Tattooed) [Live]"), guest("Sloth  [Live]", &["Mood Tattooed"]));
        assert_eq!(split("Sloth (feat. A) (ft. B)"), guest("Sloth", &["A", "B"]));
        assert_eq!(split("Defeat Of Ft Lauderdale"), guest("Defeat Of Ft Lauderdale", &[]));
        assert_eq!(split("Feat of Clay"), guest("Feat of Clay", &[]), "a word that leaves no name is the name");
        assert_eq!(split("(feat. Nobody)"), guest("(feat. Nobody)", &[]));
        assert_eq!(split("Ålborg (feat. Bjørk)"), guest("Ålborg", &["Bjørk"]));
    }

    /// The real library's Agoria: every length and the release agree, the Music copy names the guest and the Incoming copy
    /// does not.
    #[test]
    fn a_guest_named_on_one_copy_is_the_same_track() {
        let plain = rec(Some("r1"), "Agoria", "Drift", &[("Embrace", 300_000), ("Scala", 240_000)]);
        let named = rec(Some("r1"), "Agoria", "Drift", &[("Embrace (feat. Phoebe Killdeer)", 300_100), ("Scala", 240_000)]);
        assert!(same_record(&plain, &named));
        assert!(same_record(&named, &plain), "either way round");
        let both = rec(Some("r1"), "Agoria", "Drift", &[("Embrace ft. Phoebe Killdeer", 300_000), ("Scala", 240_000)]);
        assert!(same_record(&named, &both), "the same guest, written otherwise");
        let longer = rec(Some("r1"), "Agoria", "Drift", &[("Embrace (feat. Phoebe Killdeer)", 310_000), ("Scala", 240_000)]);
        assert!(!same_record(&plain, &longer), "a length still decides");
    }

    #[test]
    fn two_different_guests_are_two_tracks() {
        let one = rec(None, "A", "B", &[("Song (feat. X)", 200_000)]);
        assert!(!same_record(&one, &rec(None, "A", "B", &[("Song (feat. Y)", 200_000)])));
        assert!(!same_record(&one, &rec(None, "A", "B", &[("Song (feat. X & Y)", 200_000)])), "another list of guests");
        assert!(same_record(&rec(None, "A", "B", &[("Song (feat. X & Y)", 200_000)]), &rec(None, "A", "B", &[("Song feat. Y, X", 200_000)])), "in another order");
    }

    fn f(name: &str, ms: i64, recording: Option<&str>, title: Option<&str>, artist: Option<&str>) -> FileFacts {
        FileFacts {
            folder: 0,
            track: None,
            name: name_of(name),
            duration_ms: Some(ms),
            size: None,
            mtime: None,
            recording: recording.map(fold),
            title: title.map(fold),
            artist: artist.map(fold),
        }
    }

    /// A file as a walk found it: its size and modified time beside its name and length.
    fn on_disk(name: &str, ms: i64, size: i64, mtime: i64) -> FileFacts {
        FileFacts { size: Some(size), mtime: Some(mtime), ..f(name, ms, None, None, None) }
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

    /// A2: a file with no tags to go by and another name, which no other rule can follow: its size, time and length say it is that file.
    #[test]
    fn a_file_moved_and_renamed_with_nothing_to_go_by_pairs_as_itself() {
        let new = vec![on_disk("x/track one.flac", 200_000, 4_000_000, 1_700_000_000)];
        let gone = vec![on_disk("y/01.flac", 200_000, 4_000_000, 1_700_000_000), on_disk("y/02.flac", 200_000, 4_000_001, 1_700_000_000)];
        assert_eq!(match_moves(&new, &gone), vec![(0, 0)]);
        assert!(match_moves(&[on_disk("x/track one.flac", 200_000, 4_000_000, 1_700_000_001)], &gone[..1]).is_empty(), "another time: written since");
        assert!(match_moves(&[on_disk("x/track one.flac", 200_000, 4_000_001, 1_700_000_000)], &gone[..1]).is_empty(), "another size");
        assert!(match_moves(&[on_disk("x/track one.flac", 200_500, 4_000_000, 1_700_000_000)], &gone[..1]).is_empty(), "another length, within what a rule forgives");
    }

    /// Asked first, it beats a fit by a weaker rule, which is what it is for: a file moved to where another's name is.
    #[test]
    fn the_file_itself_beats_a_fit_by_name() {
        let new = vec![FileFacts { size: Some(4_000_000), mtime: Some(1_700_000_000), ..f("c/01 Intro.flac", 60_000, None, Some("Intro"), None) }];
        let namesake = FileFacts { size: Some(5_000_000), mtime: Some(1_600_000_000), ..f("a/01 Intro.flac", 60_000, None, Some("Intro"), None) };
        let itself = FileFacts { size: Some(4_000_000), mtime: Some(1_700_000_000), ..f("b/Intro take.flac", 60_000, None, Some("Intro"), None) };
        assert_eq!(match_moves(&new, &[namesake, itself]), vec![(0, 1)]);
    }

    /// Two identical copies are two fits, as under any rule: the first to take one would be a guess.
    #[test]
    fn identical_copies_are_not_told_apart_by_being_identical() {
        let copy = |name: &str| on_disk(name, 200_000, 4_000_000, 1_700_000_000);
        assert!(match_moves(&[copy("c/1.flac")], &[copy("a/2.flac"), copy("b/3.flac")]).is_empty(), "two gone, one new");
        assert!(match_moves(&[copy("c/1.flac"), copy("d/1.flac")], &[copy("a/2.flac")]).is_empty(), "one gone, two new");
    }

    /// What has no file any more has no size to fit: a retired track, as `retired_like` reads it, or a file not read to its length.
    #[test]
    fn nothing_fits_exactly_without_a_size_a_time_and_a_length() {
        let retired = f("a/1.flac", 200_000, None, None, None);
        assert!(match_moves(&[on_disk("x/2.flac", 200_000, 1, 1)], &[retired]).is_empty());
        let (mut unread, mut gone) = (on_disk("x/2.flac", 0, 1, 1), on_disk("a/1.flac", 0, 1, 1));
        (unread.duration_ms, gone.duration_ms) = (None, None);
        assert!(match_moves(&[unread], &[gone]).is_empty());
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
