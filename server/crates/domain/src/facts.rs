//! A record's facts with their sources (SCENARIOS-VR01; SCENARIOS-DC1 FA1-FA8, FA13; DOMAIN-CORE §10, D.1): the tracklists every
//! source keeps, how two of them agree, the versions they make with none chosen, and what the year and the type resolve to. Pure:
//! the store reads the lists and the sources' values, and hands them here.
//!
//! - **One measure of agreeing** ([`agree`], VR2): X3's title rule both ways and the one length rule, track by track in order, as
//!   many tracks. A list against a copy's files pairs by [`pairs_with`] (`assign`, the same title rule and length rule).
//! - **A guess** (VR8, the owner's 02:32 rule): a reference found by a name search alone that nothing confirms. Today's one
//!   pairing confirms it ([`confirms`]: a copy whose files and the list pair one for one, both ways, lengths within the slack);
//!   no stored label does. A guess is no version, judges nothing, and its year and type count in nothing ([`resolve_year`],
//!   [`resolve_type`]).
//! - **Versions, none chosen** ([`versions`], VR3, VR4; decision 0105): lists that agree are one version naming each source; lists
//!   that differ are versions, none chosen by order; a list without lengths is named on every version it agrees with (OQ-VR-8
//!   (a)). An administrator's pick ([`Reference`]) is the one chosen.
//! - **What judges whole** ([`judging`], VR12 rule 1, VR14, VR21): the pick alone where there is one; else every list that is no
//!   guess, and of those only the timed ones while any is timed (RV-21). Only guesses kept: not known whole (rule 6, C16).

use std::collections::BTreeMap;

use cratarr_rules::assign::{Fit, Side, assign, fit};
use serde::{Deserialize, Serialize};

use crate::identity::titled_as;

/// The sources in the order they are asked (decision 0105 rule 1: the order says only who is asked first, never who wins).
pub const ASKED: [&str; 3] = ["musicbrainz", "discogs", "bandcamp"];

/// A source as the page names it (VR3's "9 tracks · MusicBrainz, Bandcamp").
pub fn source_word(source: &str) -> &str {
    match source {
        "musicbrainz" => "MusicBrainz",
        "discogs" => "Discogs",
        "bandcamp" => "Bandcamp",
        other => other,
    }
}

/// A source's place in [`ASKED`]: another source after them, by name.
pub fn asked_at(source: &str) -> usize {
    ASKED.iter().position(|s| *s == source).unwrap_or(ASKED.len())
}

/// One track of a tracklist a source gives (moved down from the store, L6): its disc and place, title, length in milliseconds
/// where the source gives one, and its recording where it names one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "ListTrack"))]
pub struct RefTrack {
    pub disc: u32,
    pub position: u32,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub length_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub recording: Option<String>,
}

/// A reference a list is kept by (VR7 (b), H5): its source, kind (`release`, `release-group`, `master`, `album`) and value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "ListReference"))]
pub struct Reference {
    pub source: String,
    pub kind: String,
    pub value: String,
}

impl Reference {
    pub fn new(source: &str, kind: &str, value: &str) -> Self {
        Self { source: source.to_string(), kind: kind.to_string(), value: value.to_string() }
    }
}

/// One kept list (`reference_tracks`' row): the reference it was read for, its tracks, when, and whether it is a guess's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    pub reference: Reference,
    pub tracks: Vec<RefTrack>,
    pub at: i64,
    pub guessed: bool,
}

/// One present file of a copy as the pairing reads it: its place (no disc is disc 1), a key giving one fixed order (its path),
/// its title (its tag, else its file's name: `TrackFacts::of`) and the length the library serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Present {
    pub disc: Option<u32>,
    pub number: Option<u32>,
    pub key: String,
    pub title: String,
    pub ms: Option<i64>,
}

/// A list's tracks and a copy's files paired one for one (`assign`, X3's title rule, the one length rule): (track, file) pairs,
/// sorted by track. The holding, F53's arrival row and the pairing that confirms a match read this one.
pub fn pairs_with(list: &[RefTrack], files: &[Present]) -> Vec<(usize, usize)> {
    let tracks: Vec<Side> = list.iter().map(|t| Side { disc: Some(t.disc), number: Some(t.position), key: &t.title, length_ms: t.length_ms }).collect();
    let sides: Vec<Side> = files.iter().map(|f| Side { disc: f.disc, number: f.number, key: &f.key, length_ms: f.ms }).collect();
    assign(&tracks, &sides, |t, f| if titled_as(&files[f].title, &list[t].title) { fit(list[t].length_ms, files[f].ms) } else { Fit::No })
}

/// How many of a list's tracks a copy's files pair with, one for one ([`pairs_with`]).
pub fn paired_with(list: &[RefTrack], files: &[Present]) -> usize {
    pairs_with(list, files).len()
}

/// Whether a copy's files confirm a list (RV-22, the owner's 02:32 and "both ways"): every file and every track of the list pair
/// one for one, titles by X3's rule, each length within the slack (a length unknown on either side confirms nothing). A
/// standard edition never confirms a deluxe release, nor a partial copy its whole list.
pub fn confirms(list: &[RefTrack], files: &[Present]) -> bool {
    if list.is_empty() || list.len() != files.len() {
        return false;
    }
    let pairs = pairs_with(list, files);
    pairs.len() == list.len() && pairs.iter().all(|&(t, f)| matches!(fit(list[t].length_ms, files[f].ms), Fit::Within(_)))
}

/// Whether a list gives lengths (a timed list, VR21).
pub fn timed(list: &[RefTrack]) -> bool {
    list.iter().any(|t| t.length_ms.is_some())
}

/// Whether two lists agree (VR2, H8, RV-8): as many tracks, in order, each title the other's by X3's rule both ways, each length
/// within the slack where both give one. Symmetric. An empty list says nothing, and agrees with none.
pub fn agree(a: &[RefTrack], b: &[RefTrack]) -> bool {
    !a.is_empty()
        && a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| titled_as(&x.title, &y.title) && titled_as(&y.title, &x.title) && fit(x.length_ms, y.length_ms) != Fit::No)
}

/// One version of a record's tracklist (VR3, VR4): its tracks, the sources that give it (in the order asked, each once) and the
/// references they were read for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListVersion {
    pub tracks: Vec<RefTrack>,
    pub by: Vec<String>,
    pub refs: Vec<Reference>,
}

impl ListVersion {
    fn of(list: &List) -> Self {
        Self { tracks: list.tracks.clone(), by: vec![list.reference.source.clone()], refs: vec![list.reference.clone()] }
    }

    fn name(&mut self, list: &List) {
        if !self.by.contains(&list.reference.source) {
            self.by.push(list.reference.source.clone());
            self.by.sort_by_key(|s| (asked_at(s), s.clone()));
        }
        self.refs.push(list.reference.clone());
    }

    /// Whether it holds this reference's list.
    pub fn holds(&self, reference: &Reference) -> bool {
        self.refs.contains(reference)
    }
}

/// The lists in the order the versions are made: by the order asked, then by reference. Never an order that marks one.
fn ordered(lists: &[List]) -> Vec<&List> {
    let mut lists: Vec<&List> = lists.iter().filter(|l| !l.guessed && !l.tracks.is_empty()).collect();
    lists.sort_by(|a, b| (asked_at(&a.reference.source), &a.reference).cmp(&(asked_at(&b.reference.source), &b.reference)));
    lists
}

/// The versions a record's kept lists make (VR3, VR4; OQ-VR-5 (b), OQ-VR-8 (a)): lists that agree are one version, each source
/// named; a list without lengths named on every timed version it agrees with, else one of its own (or with other untimed lists
/// it agrees with). A guess makes none (VR8), nor an empty list (VR2 (g)). Shortest first; none of them chosen.
pub fn versions(lists: &[List]) -> Vec<ListVersion> {
    let lists = ordered(lists);
    let mut out: Vec<ListVersion> = Vec::new();
    for list in lists.iter().filter(|l| timed(&l.tracks)) {
        match out.iter_mut().find(|v| agree(&v.tracks, &list.tracks)) {
            Some(v) => v.name(list),
            None => out.push(ListVersion::of(list)),
        }
    }
    let timed_versions = out.len();
    for list in lists.iter().filter(|l| !timed(&l.tracks)) {
        let mut named = false;
        for v in out[..timed_versions].iter_mut().filter(|v| agree(&v.tracks, &list.tracks)) {
            v.name(list);
            named = true;
        }
        if named {
            continue;
        }
        match out[timed_versions..].iter_mut().find(|v| agree(&v.tracks, &list.tracks)) {
            Some(v) => v.name(list),
            None => out.push(ListVersion::of(list)),
        }
    }
    out.sort_by_key(|v| (v.tracks.len(), asked_at(&v.by[0])));
    out
}

/// Which lists judge a copy whole (VR12 rule 1, VR14, VR21, VR8): the pick's alone where one is picked and kept; else every kept
/// list that is no guess (a guess the pairing confirmed is none), and of those the timed ones alone while any is timed (RV-21).
/// Empty: no list judges (none kept, or only guesses: [`only_guesses`]).
pub fn judging<'a>(lists: &'a [List], pick: Option<&Reference>) -> Vec<&'a List> {
    if let Some(picked) = pick.and_then(|p| lists.iter().find(|l| &l.reference == p && !l.tracks.is_empty())) {
        return vec![picked];
    }
    let sure: Vec<&List> = lists.iter().filter(|l| !l.guessed && !l.tracks.is_empty()).collect();
    if sure.iter().any(|l| timed(&l.tracks)) { sure.into_iter().filter(|l| timed(&l.tracks)).collect() } else { sure }
}

/// Whether the only lists kept are guesses' (VR12 rule 6, C16): the record is not known whole.
pub fn only_guesses(lists: &[List], pick: Option<&Reference>) -> bool {
    judging(lists, pick).is_empty() && lists.iter().any(|l| l.guessed && !l.tracks.is_empty())
}

/// The tracklist as the view carries it (VR38, FA1, FA3-FA6): the list shown and the sources it is by (marked "· Tracklist"),
/// and the versions where none is chosen. A had record's list is its files' (`by` its copy, "files@MUSIC"), unmarked, with no
/// versions (FA3, OQ-DC1-4 (a)); once an administrator picks, the pick's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tracklist {
    pub value: Option<Vec<RefTrack>>,
    pub by: Vec<String>,
    pub versions: Vec<ListVersion>,
}

/// The tracklist fact (VR38): `files` a had record's deciding copy's list and its `files@<library>`; `lists` every list kept;
/// `pick` the administrator's.
pub fn tracklist(files: Option<(Vec<RefTrack>, String)>, lists: &[List], pick: Option<&Reference>) -> Tracklist {
    let all = versions(lists);
    let picked = pick.and_then(|p| {
        lists.iter().find(|l| &l.reference == p && !l.tracks.is_empty()).map(|l| match all.iter().find(|v| v.holds(p)) {
            Some(v) => v.clone(),
            None => ListVersion::of(l),
        })
    });
    match (files, picked) {
        (_, Some(p)) => {
            let mut rest: Vec<ListVersion> = if lists.is_empty() { Vec::new() } else { all.into_iter().filter(|v| v.refs != p.refs).collect() };
            let by = p.by.clone();
            let value = Some(p.tracks.clone());
            rest.insert(0, p);
            Tracklist { value, by, versions: rest }
        }
        (Some((files, by)), None) => Tracklist { value: Some(files), by: vec![by], versions: Vec::new() },
        (None, None) if all.len() == 1 => {
            let v = &all[0];
            Tracklist { value: Some(v.tracks.clone()), by: v.by.clone(), versions: Vec::new() }
        }
        (None, None) => Tracklist { value: None, by: Vec::new(), versions: all },
    }
}

/// The sources marked "· Tracklist" (FA3-FA6): the tracklist's `by`, but a copy's files, which mark nothing.
pub fn tracklist_mark(t: &Tracklist) -> Vec<String> {
    t.by.iter().filter(|b| !b.starts_with("files@")).cloned().collect()
}

/// One source's value of a fact (FA15): the reference it was read for (a year lives on its reference, H4; none for a fact no
/// reference gives), the source, its value (none: asked, none found, H3), and whether that reference is a guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said<T> {
    pub reference: Option<Reference>,
    pub source: String,
    pub value: Option<T>,
    pub guessed: bool,
}

/// A copy's files' values of a fact (FA2): which library's copy (its `files@` name), what it is to the record (a main library's
/// first, then Incoming, then someone's own music: `rank` 0, 1, 2), when it was added, and each file's value in track order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopySays<T> {
    pub by: String,
    pub rank: u8,
    pub added: i64,
    pub values: Vec<Option<T>>,
}

/// What the files say of a fact (FA2, M15, T-COU-12): the deciding copy's, the first that says anything of a main library's
/// copies, then Incoming's, then the first own copy added; within it, its files by majority, a tie to the first in track order.
/// With its `files@<library>`.
pub fn files_say<T: Clone + Eq>(copies: &[CopySays<T>]) -> Option<(T, String)> {
    let mut copies: Vec<&CopySays<T>> = copies.iter().filter(|c| c.values.iter().any(Option::is_some)).collect();
    copies.sort_by_key(|c| (c.rank, c.added));
    let copy = copies.first()?;
    let said: Vec<&T> = copy.values.iter().flatten().collect();
    let count = |v: &T| said.iter().filter(|x| ***x == *v).count();
    let mut best: Option<(&T, usize)> = None;
    for &v in &said {
        let n = count(v);
        if best.is_none_or(|(_, most)| n > most) {
            best = Some((v, n));
        }
    }
    best.map(|(v, _)| (v.clone(), copy.by.clone()))
}

/// One value a fact may take and the sources that say it (a version of the fact, FA13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Valued<T> {
    pub value: T,
    pub by: Vec<String>,
}

/// A fact resolved (FA1, FA13): its value and what it is by, or none and its versions where its sources differ, none chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved<T> {
    pub value: Option<T>,
    pub by: Vec<String>,
    pub versions: Vec<Valued<T>>,
}

fn named(mut by: Vec<String>) -> Vec<String> {
    by.sort_by_key(|s| (asked_at(s), s.clone()));
    by.dedup();
    by
}

/// The services' values of a fact as equals (decision 0105): each value no guess gives, with its sources, ordered by the value
/// itself, never by the order asked.
fn values_of<T: Clone + Ord>(said: &[Said<T>]) -> Vec<Valued<T>> {
    let mut out: Vec<Valued<T>> = Vec::new();
    for s in said.iter().filter(|s| !s.guessed) {
        let Some(value) = &s.value else { continue };
        match out.iter_mut().find(|v| &v.value == value) {
            Some(v) => v.by.push(s.source.clone()),
            None => out.push(Valued { value: value.clone(), by: vec![s.source.clone()] }),
        }
    }
    for v in &mut out {
        v.by = named(std::mem::take(&mut v.by));
    }
    out.sort_by(|a, b| a.value.cmp(&b.value));
    out
}

/// The year (FA1, FA2, FA13; VR39; OQ-VR-9 (c)): the hand's; else the deciding copy's files' (`files`), with every service that
/// says the same; else the services as equals, each release's year on its reference (H4): one value they all say, or the picked
/// reference's, else none with its versions until a hand picks. A guess's year never counts (M4); "asked, none" says nothing.
pub fn resolve_year(hand: Option<i32>, files: Option<(i32, String)>, said: &[Said<i32>], pick: Option<&Reference>) -> Resolved<i32> {
    if let Some(year) = hand {
        return Resolved { value: Some(year), by: vec!["hand".to_string()], versions: Vec::new() };
    }
    let values = values_of(said);
    if let Some((year, by)) = files {
        let mut by = vec![by];
        by.extend(values.iter().filter(|v| v.value == year).flat_map(|v| v.by.clone()));
        return Resolved { value: Some(year), by, versions: Vec::new() };
    }
    let picked = pick.and_then(|p| said.iter().find(|s| s.reference.as_ref() == Some(p)).and_then(|s| s.value));
    if let Some(year) = picked {
        let by = values.iter().find(|v| v.value == year).map_or_else(Vec::new, |v| v.by.clone());
        return Resolved { value: Some(year), by, versions: values };
    }
    match values.len() {
        0 => Resolved { value: None, by: Vec::new(), versions: Vec::new() },
        1 => Resolved { value: Some(values[0].value), by: values[0].by.clone(), versions: Vec::new() },
        _ => Resolved { value: None, by: Vec::new(), versions: values },
    }
}

/// What the type is by (FA13, VR39): the hand's; the release type rule's over a had record's deciding copy (`rule`) or over the
/// picked list ("pick"); else its sources by majority.
pub const BY_HAND: &str = "hand";
pub const BY_RULE: &str = "rule";
pub const BY_PICK: &str = "pick";

/// The type's why, as the page says it (FA1, VR39).
pub fn type_why(by: &[String]) -> &'static str {
    match by.first().map(String::as_str) {
        Some(BY_HAND) => "set by you",
        Some(BY_RULE) => "by the release type rule",
        Some(BY_PICK) => "by the picked list",
        Some(_) => "by its sources",
        None => "",
    }
}

/// The type (FA13; VR39, M10, RV-12; the rule itself is VERSIONS'): the hand's; a had record's by the release type rule over its
/// deciding copy (`had_rule`), never its files' type tag; a record not had's by the rule over its picked list (`picked_rule`), else
/// its sources' type facts where they all agree, else none until a hand picks, as the year (the owner, 2026-11-21 13:59: no
/// majority). A guess's type never counts (M4).
pub fn resolve_type(hand: Option<&str>, had_rule: Option<&str>, picked_rule: Option<&str>, said: &[Said<String>]) -> Resolved<String> {
    let one = |value: &str, by: &str| Resolved { value: Some(value.to_string()), by: vec![by.to_string()], versions: Vec::new() };
    if let Some(t) = hand {
        return one(t, BY_HAND);
    }
    if let Some(t) = had_rule {
        return one(t, BY_RULE);
    }
    if let Some(t) = picked_rule {
        return one(t, BY_PICK);
    }
    match values_of(said).as_slice() {
        [v] => Resolved { value: Some(v.value.clone()), by: v.by.clone(), versions: Vec::new() },
        _ => Resolved { value: None, by: Vec::new(), versions: Vec::new() },
    }
}

/// The tracks of a list each source keeps, by source (a test's and a report's view of `versions`).
pub fn by_source(lists: &[List]) -> BTreeMap<String, usize> {
    lists.iter().map(|l| (l.reference.source.clone(), l.tracks.len())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(position: u32, title: &str, secs: Option<i64>) -> RefTrack {
        RefTrack { disc: 1, position, title: title.to_string(), length_ms: secs.map(|s| s * 1000), recording: None }
    }

    /// Salt Lines' ten (S1-S10), lengths 200 s + n × 7 s.
    fn salt(n: u32) -> Vec<RefTrack> {
        (1..=n).map(|i| t(i, &format!("S{i}"), Some(200 + i64::from(i) * 7))).collect()
    }

    fn untimed(n: u32) -> Vec<RefTrack> {
        (1..=n).map(|i| t(i, &format!("S{i}"), None)).collect()
    }

    fn list(source: &str, kind: &str, value: &str, tracks: Vec<RefTrack>) -> List {
        List { reference: Reference::new(source, kind, value), tracks, at: 1, guessed: false }
    }

    fn guess(source: &str, value: &str, tracks: Vec<RefTrack>) -> List {
        List { guessed: true, ..list(source, "album", value, tracks) }
    }

    fn file(n: u32, title: &str, ms: Option<i64>) -> Present {
        Present { disc: Some(1), number: Some(n), key: format!("{n:02}.flac"), title: title.to_string(), ms }
    }

    fn copy_of(tracks: &[RefTrack]) -> Vec<Present> {
        tracks.iter().map(|t| file(t.position, &t.title, t.length_ms)).collect()
    }

    fn names(by: &[String]) -> Vec<&str> {
        by.iter().map(String::as_str).collect()
    }

    /// VR2 (H8, RV-8): one rule says when two lists agree: X3's title rule both ways with the one length rule, track by track in
    /// order. Red first: no `agree` (store's `identity::agrees` folds bare titles, so (h) differs there).
    #[test]
    fn two_lists_agree_by_one_rule() {
        let mb = salt(10);
        // (a) ten titles alike, lengths within 1 s.
        let mut bc = salt(10);
        bc.iter_mut().for_each(|x| *x.length_ms.as_mut().unwrap() += 1_000);
        assert!(agree(&mb, &bc) && agree(&bc, &mb), "(a)");
        // (b) one naming a guest.
        let mut guest = salt(10);
        guest[3].title = "S4 (feat. Ana Bell)".into();
        assert!(agree(&mb, &guest) && agree(&guest, &mb), "(b) one side names no guest");
        // (c) S7 2.5 s apart.
        let mut late = salt(10);
        *late[6].length_ms.as_mut().unwrap() += 2_500;
        assert!(!agree(&mb, &late) && !agree(&late, &mb), "(c) past 2 000 ms");
        // (d) another order.
        let mut turned = salt(10);
        turned.reverse();
        assert!(!agree(&mb, &turned), "(d) in order");
        // (e) one track more.
        assert!(!agree(&mb, &salt(11)), "(e)");
        // (f) Discogs' ten without lengths, titles alike.
        assert!(agree(&mb, &untimed(10)) && agree(&untimed(10), &mb), "(f) on titles (OQ8 (a))");
        // (g) an empty list says nothing and makes no version.
        assert!(!agree(&[], &[]) && !agree(&mb, &[]));
        assert!(versions(&[list("discogs", "release", "D70", Vec::new())]).is_empty(), "(g) no version");
        // (h) "(Remastered)" and "(Bonus Track)" name no version: the same title.
        let mut brackets = salt(10);
        brackets[3].title = "S4 (Remastered)".into();
        brackets[9].title = "S10 (Bonus Track)".into();
        assert!(agree(&mb, &brackets) && agree(&brackets, &mb), "(h) OQ-PAIR01-1 (a)");
        // (i) "T10" against "T10 (Edit)", no lengths: differ both ways, though a file "T10" pairs with the list's "T10 (Edit)".
        let tide = |last: &str| (1..=10).map(|i| t(i, &if i == 10 { last.to_string() } else { format!("T{i}") }, None)).collect::<Vec<_>>();
        let (plain, edit) = (tide("T10"), tide("T10 (Edit)"));
        assert!(!agree(&plain, &edit) && !agree(&edit, &plain), "(i) both ways");
        assert_eq!(agree(&plain, &edit), agree(&edit, &plain));
        assert!(titled_as("T10", "T10 (Edit)"), "a version word on the list's side only still pairs a file");
        assert_eq!(paired_with(&edit, &copy_of(&plain)), 10, "the file side pairs");
        // Symmetric over every pair above.
        for (a, b) in [(&mb, &bc), (&mb, &guest), (&mb, &late), (&mb, &brackets), (&plain, &edit)] {
            assert_eq!(agree(a, b), agree(b, a));
        }
    }

    /// VR2's OQ-VR-8 (a): a list without lengths between two versions that differ by a length is named on both. Red first: no
    /// versions.
    #[test]
    fn a_list_without_lengths_between_two_versions() {
        let mb = salt(10);
        let mut bc = salt(10);
        *bc[6].length_ms.as_mut().unwrap() += 5_000;
        let lists = [list("musicbrainz", "release", "R70", mb), list("bandcamp", "album", "P70", bc), list("discogs", "release", "D70", untimed(10))];
        let vs = versions(&lists);
        assert_eq!(vs.len(), 2, "two versions, Discogs no version of its own: {vs:?}");
        assert_eq!(vs.iter().map(|v| names(&v.by)).collect::<Vec<_>>(), [vec!["musicbrainz", "discogs"], vec!["discogs", "bandcamp"]]);
        assert!(vs.iter().all(|v| v.tracks.iter().all(|x| x.length_ms.is_some())), "each version's tracks are its timed list's");
    }

    /// VR3 and FA4: sources that agree are one version, each named; a source's "asked, none" is no version (its store half is
    /// VR40's). Red first: no versions; the read stopped at the first list and marked it alone.
    #[test]
    fn agreeing_sources_are_one_version_each_named() {
        let answered = salt(9);
        let mut bc = salt(9);
        bc.iter_mut().for_each(|x| *x.length_ms.as_mut().unwrap() -= 1_500);
        let lists = [list("bandcamp", "album", "P7", bc), list("musicbrainz", "release", "R7", answered.clone())];
        let vs = versions(&lists);
        assert_eq!(vs.len(), 1);
        assert_eq!(names(&vs[0].by), ["musicbrainz", "bandcamp"], "both named, in the order asked, never only the first asked");
        let shown = tracklist(None, &lists, None);
        assert_eq!(tracklist_mark(&shown), ["musicbrainz", "bandcamp"], "both chips marked");
        assert_eq!(shown.value.as_ref().map(Vec::len), Some(9), "one list shows");
        assert!(shown.versions.is_empty(), "a record of one version has no Versions section (RS4)");
    }

    /// VR4 and FA5: sources that disagree are versions, none chosen, none marked; the list clash (b) judges by is the shortest.
    /// Red first: no versions (the first list read counted).
    #[test]
    fn disagreeing_lists_are_versions_none_chosen() {
        let lists = [
            list("musicbrainz", "release", "R7", salt(9)),
            list("bandcamp", "album", "P7", salt(9)),
            list("discogs", "release", "D7", salt(11)),
        ];
        let shown = tracklist(None, &lists, None);
        assert_eq!(shown.value, None, "no chosen value, no list of its own (OQ-VR-2 (a))");
        assert!(tracklist_mark(&shown).is_empty(), "no chip marked (FA5)");
        let lines: Vec<(usize, Vec<&str>)> = shown.versions.iter().map(|v| (v.tracks.len(), names(&v.by))).collect();
        assert_eq!(lines, [(9, vec!["musicbrainz", "bandcamp"]), (11, vec!["discogs"])]);
        // Asked in another order, the same versions: none marked by the order asked.
        let mut turned = lists.clone();
        turned.reverse();
        assert_eq!(tracklist(None, &turned, None), shown);
        // Every list judges (none picked, none a guess); the shortest is the nine.
        let judges = judging(&lists, None);
        assert_eq!(judges.len(), 3);
        assert_eq!(judges.iter().map(|l| l.tracks.len()).min(), Some(9));
    }

    /// VR38 and FA1: a fact as the view carries it: value, by, versions; had, agreeing, disagreeing and picked. Red first: no
    /// tracklist fact (the record view's `FactView` has value and by alone, by empty but for the hand).
    #[test]
    fn a_fact_is_value_by_versions_files() {
        let nine = salt(9);
        let agreeing = [list("musicbrainz", "release", "R7", nine.clone()), list("bandcamp", "album", "P7", nine.clone())];
        let a = tracklist(None, &agreeing, None);
        assert_eq!((a.value.clone(), names(&a.by), a.versions.len()), (Some(nine.clone()), vec!["musicbrainz", "bandcamp"], 0));
        let disagreeing = [agreeing[0].clone(), agreeing[1].clone(), list("discogs", "release", "D7", salt(11))];
        let d = tracklist(None, &disagreeing, None);
        assert_eq!((d.value.clone(), d.by.len(), d.versions.len()), (None, 0, 2));
        // Picked: the picked version as the value, by the picked source and every source agreeing with it, shown first.
        let picked = Reference::new("discogs", "release", "D7");
        let p = tracklist(None, &disagreeing, Some(&picked));
        assert_eq!((p.value.as_ref().map(Vec::len), names(&p.by)), (Some(11), vec!["discogs"]));
        assert_eq!(p.versions.iter().map(|v| v.tracks.len()).collect::<Vec<_>>(), [11, 9], "the pick first, the other in versions");
        let mb = Reference::new("musicbrainz", "release", "R7");
        assert_eq!(names(&tracklist(None, &disagreeing, Some(&mb)).by), ["musicbrainz", "bandcamp"], "every source agreeing with the pick");
        // Had: the files', by its copy, no versions, no mark.
        let h = tracklist(Some((nine.clone(), "files@MUSIC".to_string())), &disagreeing, None);
        assert_eq!((h.value, names(&h.by), h.versions.len()), (Some(nine), vec!["files@MUSIC"], 0));
        assert!(tracklist_mark(&tracklist(Some((salt(10), "files@MUSIC".into())), &disagreeing, None)).is_empty());
        // The year as FA1 gives it: the files decide, a service that says the same is named; Discogs' 2015 kept, no version.
        let said = [
            Said { reference: Some(mb.clone()), source: "musicbrainz".into(), value: Some(2014), guessed: false },
            Said { reference: Some(picked.clone()), source: "discogs".into(), value: Some(2015), guessed: false },
        ];
        let y = resolve_year(None, Some((2014, "files@MUSIC".into())), &said, None);
        assert_eq!((y.value, names(&y.by), y.versions.len()), (Some(2014), vec!["files@MUSIC", "musicbrainz"], 0));
        // The type by the rule, said so.
        let ty = resolve_type(None, Some("album"), None, &[]);
        assert_eq!((ty.value.as_deref(), names(&ty.by), type_why(&ty.by)), (Some("album"), vec!["rule"], "by the release type rule"));
    }

    /// FA3 (VR38's had row, VR34): a had record's list is its files', unmarked, whatever the services keep, until a pick. Red
    /// first: no tracklist mark.
    #[test]
    fn a_had_records_list_is_its_files_unmarked() {
        let lists = [list("musicbrainz", "release", "R42", salt(10)), list("discogs", "release", "D42", salt(12))];
        let had = tracklist(Some((salt(10), "files@MUSIC".into())), &lists, None);
        assert!(tracklist_mark(&had).is_empty(), "no chip marked without a pick");
        assert_eq!(had.value.as_ref().map(Vec::len), Some(10), "the files' list");
        assert!(had.versions.is_empty(), "no Tracklist line on a had record");
        // FA6: once an administrator picks Discogs' list, the pick and every source agreeing with it are marked; after Reset, none.
        let agreeing = [list("musicbrainz", "release", "R42", salt(12)), list("discogs", "release", "D42", salt(12))];
        let picked = tracklist(Some((salt(10), "files@MUSIC".into())), &agreeing, Some(&Reference::new("discogs", "release", "D42")));
        assert_eq!(tracklist_mark(&picked), ["musicbrainz", "discogs"]);
        assert!(tracklist_mark(&tracklist(Some((salt(10), "files@MUSIC".into())), &agreeing, None)).is_empty(), "after Reset");
    }

    /// VR8 (the domain half): a guess is no version and judges nothing until confirmed; a record whose only lists are guesses is
    /// not known whole (C16). Today's one pairing confirms it, both ways (RV-22). Red first: every kept list judged.
    #[test]
    fn a_guessed_list_waits_for_its_confirmation() {
        let lists = [list("musicbrainz", "release", "R70", salt(10)), list("bandcamp", "album", "P70", salt(10)), guess("bandcamp", "P70s", salt(7))];
        assert_eq!(versions(&lists).len(), 1, "the guess makes no version: {:?}", versions(&lists));
        assert!(judging(&lists, None).iter().all(|l| !l.guessed), "never judged by");
        assert_eq!(judging(&lists, None).iter().map(|l| l.tracks.len()).min(), Some(10), "the shortest stays 10, never 7");
        assert!(!only_guesses(&lists, None));
        // Only guesses kept: not known whole, and no list judges.
        let guesses = [guess("musicbrainz", "R81", salt(10))];
        assert!(judging(&guesses, None).is_empty() && only_guesses(&guesses, None));
        // A pick of it judges (a hand).
        let r81 = Reference::new("musicbrainz", "album", "R81");
        assert_eq!(judging(&guesses, Some(&r81)).len(), 1);
        assert!(!only_guesses(&guesses, Some(&r81)));
        // Confirmed by the pairing: every file and every track one for one, within the slack, both ways.
        let ten = salt(10);
        assert!(confirms(&ten, &copy_of(&ten)), "(a) one for one");
        assert!(!confirms(&ten, &copy_of(&ten[..9])), "(b) a track missing");
        let mut mismatched = copy_of(&ten);
        mismatched[3].ms = Some(280_000);
        assert!(!confirms(&ten, &mismatched), "(c) a track 4:40 against its list");
        let mut extra = copy_of(&ten);
        extra.push(file(11, "Bonus", Some(100_000)));
        assert!(!confirms(&ten, &extra), "a standard edition never confirms a deluxe release: both ways");
        assert!(!confirms(&untimed(10), &copy_of(&ten)), "a list without lengths confirms by no pairing");
        assert!(!confirms(&[], &[]), "no list read: nothing to confirm");
    }

    /// VR12 rule 1, VR14, VR21's rule (RV-21): the pick alone judges; else every list no guess, the timed alone while one is.
    #[test]
    fn what_judges_whole() {
        let lists = [list("discogs", "release", "D70", untimed(10)), list("musicbrainz", "release", "R70", salt(12))];
        let j = judging(&lists, None);
        assert_eq!(j.iter().map(|l| l.reference.value.as_str()).collect::<Vec<_>>(), ["R70"], "a list without lengths judges nothing beside a timed one");
        let untimed_only = [list("discogs", "release", "D70", untimed(10))];
        assert_eq!(judging(&untimed_only, None).len(), 1, "by titles alone where no list gives lengths");
        let m70 = Reference::new("discogs", "master", "M70");
        let picked = [list("discogs", "master", "M70", salt(12)), list("musicbrainz", "release", "R70", salt(10))];
        assert_eq!(judging(&picked, Some(&m70)).iter().map(|l| l.tracks.len()).collect::<Vec<_>>(), [12], "the pick alone");
        assert_eq!(judging(&picked, Some(&Reference::new("discogs", "release", "D-gone"))).len(), 2, "a pick not kept: the lists");
    }

    /// FA2 (M15, T-COU-12): the files are a source per copy: a main library's copy decides (the first that says anything), then
    /// Incoming's, then the first own copy added; within it, by majority, a tie to the first in track order. Carries VR39's Night
    /// Ferry row (2014 by its files; Discogs' 2015 kept, no version). Red first: no rule (the year the last copy read gives).
    #[test]
    fn the_files_are_a_source_per_copy_by_the_shared_facts_rule() {
        let copy = |by: &str, rank: u8, added: i64, values: Vec<Option<i32>>| CopySays { by: by.into(), rank, added, values };
        // L.A. Night: MUSIC's untagged MP3, IN's FLAC tagged 1971.
        let la = [copy("files@MUSIC", 0, 1, vec![None; 9]), copy("files@IN", 1, 2, vec![Some(1971); 9])];
        assert_eq!(files_say(&la), Some((1971, "files@IN".into())), "MUSIC says nothing: IN's decides");
        let tagged = [copy("files@MUSIC", 0, 1, vec![Some(1970); 9]), la[1].clone()];
        assert_eq!(files_say(&tagged), Some((1970, "files@MUSIC".into())), "once MUSIC's files say, MUSIC's decides");
        // Only in OWN-A (alice's, first) and OWN-B (bob's).
        let owns = [copy("files@OWN-B", 2, 20, vec![Some(2003)]), copy("files@OWN-A", 2, 10, vec![Some(2001)])];
        assert_eq!(files_say(&owns), Some((2001, "files@OWN-A".into())), "the first copy added");
        // MUSIC 1999 and OWN-A 2000: 1999 for everyone.
        let mixed = [copy("files@OWN-A", 2, 1, vec![Some(2000)]), copy("files@MUSIC", 0, 5, vec![Some(1999)])];
        assert_eq!(files_say(&mixed), Some((1999, "files@MUSIC".into())), "an own copy never decides for others");
        // Nine files of 1971 and one of 1972: 1971; a tie, the first in track order.
        let mut votes = vec![Some(1971); 9];
        votes.insert(4, Some(1972));
        assert_eq!(files_say(&[copy("files@IN", 1, 1, votes)]).map(|(y, _)| y), Some(1971));
        assert_eq!(files_say(&[copy("files@IN", 1, 1, vec![Some(1972), Some(1971)])]).map(|(y, _)| y), Some(1972), "a tie: the first");
        // VR39's Night Ferry: 2014 by its files; Discogs' 2015 kept, no version (the files decide).
        let said = [Said { reference: Some(Reference::new("discogs", "release", "D42")), source: "discogs".into(), value: Some(2015), guessed: false }];
        let night_ferry = resolve_year(None, files_say(&[copy("files@MUSIC", 0, 1, vec![Some(2014); 10])]), &said, None);
        assert_eq!((night_ferry.value, names(&night_ferry.by), night_ferry.versions.len()), (Some(2014), vec!["files@MUSIC"], 0));
    }

    /// VR39 and FA13: the year by the hand, then the files, then the services as equals, each release's year on its reference
    /// (H4); none until a hand picks where they differ (OQ-VR-9 (c)); a guess's year never counts (M4). Red first: one year per
    /// record, the order asked winning.
    #[test]
    fn the_year_has_versions_none_chosen() {
        let said = |source: &str, value: &str, year: Option<i32>, guessed: bool| Said {
            reference: Some(Reference::new(source, "release", value)),
            source: source.to_string(),
            value: year,
            guessed,
        };
        // FA13: MusicBrainz 2014 and Discogs 2014: 2014 by both.
        let both = [said("musicbrainz", "R1", Some(2014), false), said("discogs", "D1", Some(2014), false)];
        let y = resolve_year(None, None, &both, None);
        assert_eq!((y.value, names(&y.by)), (Some(2014), vec!["musicbrainz", "discogs"]));
        // Then Discogs 2015: two versions, none chosen, none shown.
        let apart = [said("musicbrainz", "R1", Some(2014), false), said("discogs", "D1", Some(2015), false)];
        let y = resolve_year(None, None, &apart, None);
        assert_eq!(y.value, None, "no year until a hand picks");
        assert_eq!(y.versions.iter().map(|v| (v.value, names(&v.by))).collect::<Vec<_>>(), [(2014, vec!["musicbrainz"]), (2015, vec!["discogs"])]);
        // Asked the other way round: the same (never the order asked).
        let mut turned = apart.clone();
        turned.reverse();
        assert_eq!(resolve_year(None, None, &turned, None), y);
        // VR39's Salt Lines: later Bandcamp 2014 joins MusicBrainz's version, and still none is chosen.
        let mut salt_lines = apart.to_vec();
        salt_lines.push(said("bandcamp", "P70", Some(2014), false));
        let y = resolve_year(None, None, &salt_lines, None);
        assert_eq!((y.value, y.versions[0].value, names(&y.versions[0].by)), (None, 2014, vec!["musicbrainz", "bandcamp"]));
        // FA13's Thin Walls: one source, two releases of two years: none until a pick; the pick of R2's list picks its year.
        let thin = [said("musicbrainz", "R1", Some(2014), false), said("musicbrainz", "R2", Some(2016), false)];
        assert_eq!(resolve_year(None, None, &thin, None).value, None, "two releases' years are as a tie");
        let r2 = Reference::new("musicbrainz", "release", "R2");
        let y = resolve_year(None, None, &thin, Some(&r2));
        assert_eq!((y.value, names(&y.by)), (Some(2016), vec!["musicbrainz"]));
        // The hand first; "asked, none" says nothing; a guess's year counts in nothing.
        assert_eq!(resolve_year(Some(2013), Some((2014, "files@MUSIC".into())), &apart, None).value, Some(2013));
        let none = [said("discogs", "D1", None, false), said("musicbrainz", "R1", Some(2014), false)];
        assert_eq!(resolve_year(None, None, &none, None).value, Some(2014), "asked, none is no version");
        let guessed = [said("musicbrainz", "R80", Some(2014), true), said("discogs", "D80", Some(2015), false)];
        assert_eq!(resolve_year(None, None, &guessed, None).value, Some(2015), "MusicBrainz's guessed 2014 counts in nothing");
    }

    /// VR39 (M10, RV-12): a not-had record's type is its pick's by the rule, else its sources' by majority, a tie none until a
    /// hand picks; a guess's type never counts. Red first: the type from one source by the order asked.
    #[test]
    fn a_not_had_records_type_is_its_pick_else_its_sources_most() {
        let said = |source: &str, ty: &str, guessed: bool| Said { reference: None, source: source.to_string(), value: Some(ty.to_string()), guessed };
        // Revised (the owner, 2026-11-21 13:59, "None until picked"): no majority; the sources' type where they all agree.
        let salt_lines = [said("musicbrainz", "Album", false), said("discogs", "Album", false), said("bandcamp", "EP", false)];
        assert_eq!(resolve_type(None, None, None, &salt_lines).value, None, "Album, Album, EP: no type until a hand picks");
        let agreeing = [said("musicbrainz", "Album", false), said("discogs", "Album", false), said("bandcamp", "Album", false)];
        let r = resolve_type(None, None, None, &agreeing);
        assert_eq!((r.value.as_deref(), type_why(&r.by)), (Some("Album"), "by its sources"));
        let low_ceiling = [said("musicbrainz", "EP", false), said("discogs", "Album", false)];
        assert_eq!(resolve_type(None, None, None, &low_ceiling).value, None, "one each: no type until a hand picks");
        let mut turned = low_ceiling.clone();
        turned.reverse();
        assert_eq!(resolve_type(None, None, None, &turned).value, None, "never by the order asked");
        let r = resolve_type(None, None, Some("Album"), &low_ceiling);
        assert_eq!((r.value.as_deref(), type_why(&r.by)), (Some("Album"), "by the picked list"));
        let guessed = [said("musicbrainz", "EP", true), said("discogs", "Album", false)];
        assert_eq!(resolve_type(None, None, None, &guessed).value.as_deref(), Some("Album"), "a guess's type counts in nothing");
    }

    /// FA13's type: the hand's, else the release type rule's (the store passes `release_type_of`'s answer); the files' type tag
    /// never. Red first: no `resolve_type`.
    #[test]
    fn the_type_is_the_hand_else_the_rule() {
        // The files say "ep" (MusicBrainz's word), seven distinct songs: the rule says album.
        let said = [Said { reference: None, source: "files".into(), value: Some("ep".to_string()), guessed: false }];
        assert_eq!(resolve_type(None, Some("album"), None, &said).value.as_deref(), Some("album"), "the rule, never the tag");
        let hand = resolve_type(Some("single"), Some("album"), None, &said);
        assert_eq!((hand.value.as_deref(), type_why(&hand.by)), (Some("single"), "set by you"));
        assert_eq!(resolve_type(None, Some("album"), None, &said).by, ["rule"], "after Reset, the rule's again");
    }
}
