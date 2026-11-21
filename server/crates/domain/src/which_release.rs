//! An arrival's "Which release" row (SCENARIOS-F53; PREVIEW-0.17-1 F53, the owner 2026-11-19: "0.18 with the matcher"): which of
//! a record's versions its copy is, judged by every source equally (decision 0105: none ranks above another), and the tracks that
//! differ. Pure: the store reads the copy's files, the kept lists, the pick and the guesses, and hands them here.
//!
//! - **One pairing** (F53.10): a version's tracks and the copy's files pair by [`pairs_with`], the one the holding reads. A copy is
//!   whole by a version where every track of it pairs (extra files allowed), as `whole()` reads it.
//! - **What judges** (VR14, T1; C12, RV-21): the pick alone where there is one; else every version, and of those the timed ones
//!   while any is (a list without lengths never makes a copy whole beside one with lengths).
//! - **The version named** (T-F53-6): whole by one, that one; whole by several, the pick, else none; whole by none, the closest
//!   (the fewest differences, each said once, T-F53-8), the pick among equals, else none. Never one over another by the sources'
//!   order or by which was kept first.
//! - **The row's states** (T-F53-2): Agrees (green), Closest, No list, Guessed (amber), Waiting, Unknown (red), worked out by
//!   [`row`] from what is kept; its words by [`said`], the sources in the domain's own order ([`crate::facts::ASKED`]: who is asked
//!   first, never who wins).

use cratarr_rules::assign::{Fit, fit};

use crate::facts::{ListVersion, Present, RefTrack, Reference, by_title, pairs_with, source_word, timed};
use crate::identity::{same_title, titled_as};

/// What the row says of the copy (T-F53-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Whole by a sure version (or by the pick).
    Agrees,
    /// A sure list kept, the copy whole by none.
    Closest,
    /// References kept, none of them giving a tracklist.
    NoList,
    /// No sure reference; finds by name kept, none confirmed.
    Guessed,
    /// No list kept at all, and a source with a reference not read yet, or the arrival's searches not answered.
    Waiting,
    /// No reference and no guess, the searches answered.
    Unknown,
}

impl State {
    /// The check's colour, as the card's checks say it.
    pub fn colour(self) -> &'static str {
        match self {
            State::Agrees => "green",
            State::Closest | State::NoList | State::Guessed => "amber",
            State::Waiting => "waiting",
            State::Unknown => "red",
        }
    }
}

/// Where a difference is: a track of the version (its disc and place), or a file of the copy (its key).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum At {
    Track { disc: u32, number: u32 },
    File(String),
}

/// What differs (T-F53-8): a track and a file of one title whose lengths lie past the slack; a track and a file at one place whose
/// lengths fit but whose titles don't (a version word on the file's side); a track no file pairs with; a file no track pairs with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum What {
    Length { ours: Option<i64>, theirs: Option<i64> },
    Title { ours: String, theirs: String },
    Missing,
    Extra,
}

/// One difference, said once, with the title it is said by (the track's, else the file's).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Differs {
    pub at: At,
    pub what: What,
    pub title: String,
}

/// A find by name that today's pairing doesn't confirm (VR8; the owner's 02:32): its reference and, where kept, its title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub reference: Reference,
    pub title: Option<String>,
}

/// The row (F53): its state, the version named (its place in the versions given), the sources and releases it is by, what differs,
/// the sources it waits for (of them, those that didn't answer), the guesses shown, whether the pick judged, and each file's
/// reference length (the named version's track it pairs with).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub state: State,
    pub version: Option<usize>,
    pub by: Vec<String>,
    pub releases: Vec<Reference>,
    pub differ: Vec<Differs>,
    pub waiting: Vec<String>,
    pub busy: Vec<String>,
    pub guesses: Vec<Candidate>,
    pub picked: bool,
    /// Per file of the copy, in the order given: its paired track of the version named (none where none is named or it pairs none).
    pub tracks: Vec<Option<RefTrack>>,
    /// How many tracks the version named has.
    pub of: usize,
}

impl Release {
    fn bare(state: State, files: usize) -> Release {
        Release { state, version: None, by: Vec::new(), releases: Vec::new(), differ: Vec::new(), waiting: Vec::new(), busy: Vec::new(), guesses: Vec::new(), picked: false, tracks: vec![None; files], of: 0 }
    }
}

/// The differences between a version's tracks and a copy's files, each said once (T-F53-8): first a title pair past the slack
/// (Length), then one place whose lengths fit but whose titles don't (Title), then the tracks left (Missing) and the files left
/// (Extra). In the track's order, the files after.
pub fn differences(tracks: &[RefTrack], copy: &[Present]) -> Vec<Differs> {
    let pairs = pairs_with(tracks, copy);
    let mut left_t: Vec<usize> = (0..tracks.len()).filter(|t| !pairs.iter().any(|(pt, _)| pt == t)).collect();
    let mut left_f: Vec<usize> = (0..copy.len()).filter(|f| !pairs.iter().any(|(_, pf)| pf == f)).collect();
    let at = |t: &RefTrack| At::Track { disc: t.disc, number: t.position };
    let mut out: Vec<(usize, Differs)> = Vec::new();
    for t in left_t.clone() {
        let track = &tracks[t];
        if let Some(i) = left_f.iter().position(|&f| by_title(track, &copy[f]) && fit(track.length_ms, copy[f].ms) == Fit::No) {
            let f = left_f.remove(i);
            out.push((t, Differs { at: at(track), what: What::Length { ours: copy[f].ms, theirs: track.length_ms }, title: track.title.clone() }));
            left_t.retain(|x| *x != t);
        }
    }
    for t in left_t.clone() {
        let track = &tracks[t];
        let here = |f: &Present| (f.disc.unwrap_or(1), f.number) == (track.disc, Some(track.position));
        // A version word on the file's side (OQ-VR-7 (b)): one title but for it, refused by X3's rule.
        let another = |f: &Present| same_title(&f.title, &track.title) && !titled_as(&f.title, &track.title);
        if let Some(i) = left_f.iter().position(|&f| here(&copy[f]) && another(&copy[f]) && matches!(fit(track.length_ms, copy[f].ms), Fit::Within(_))) {
            let f = left_f.remove(i);
            out.push((t, Differs { at: at(track), what: What::Title { ours: copy[f].title.clone(), theirs: track.title.clone() }, title: track.title.clone() }));
            left_t.retain(|x| *x != t);
        }
    }
    for t in left_t {
        out.push((t, Differs { at: at(&tracks[t]), what: What::Missing, title: tracks[t].title.clone() }));
    }
    out.sort_by_key(|(t, _)| *t);
    let mut out: Vec<Differs> = out.into_iter().map(|(_, d)| d).collect();
    for f in left_f {
        let title = if copy[f].title.is_empty() { copy[f].named.first().cloned().unwrap_or_else(|| copy[f].key.clone()) } else { copy[f].title.clone() };
        out.push(Differs { at: At::File(copy[f].key.clone()), what: What::Extra, title });
    }
    out
}

/// Which version a copy is (F53.1, F53.4–F53.6): `versions` the record's sure lists grouped by `agree` (VR3; a guess's never among
/// them), `pick` the administrator's, `guesses` the finds by name today's pairing doesn't confirm (shown only where no version is
/// kept). The verdict only: what waits is [`row`]'s.
pub fn which_release(copy: &[Present], versions: &[ListVersion], pick: Option<&Reference>, guesses: &[Candidate]) -> Release {
    let picked = pick.and_then(|p| versions.iter().position(|v| v.holds(p)));
    let judges: Vec<usize> = match picked {
        Some(i) => vec![i],
        None => {
            let timed_ones: Vec<usize> = (0..versions.len()).filter(|&i| timed(&versions[i].tracks)).collect();
            if timed_ones.is_empty() { (0..versions.len()).collect() } else { timed_ones }
        }
    };
    if judges.is_empty() {
        let mut r = Release::bare(if guesses.is_empty() { State::Unknown } else { State::Guessed }, copy.len());
        r.guesses = guesses.to_vec();
        return r;
    }
    let named = |state: State, i: usize, differ: Vec<Differs>| {
        let v = &versions[i];
        let pairs = pairs_with(&v.tracks, copy);
        let mut tracks = vec![None; copy.len()];
        for (t, f) in pairs {
            tracks[f] = Some(v.tracks[t].clone());
        }
        Release {
            state,
            version: Some(i),
            by: v.by.clone(),
            releases: v.refs.clone(),
            differ,
            waiting: Vec::new(),
            busy: Vec::new(),
            guesses: Vec::new(),
            picked: picked == Some(i),
            tracks,
            of: v.tracks.len(),
        }
    };
    let whole: Vec<usize> = judges.iter().copied().filter(|&i| pairs_with(&versions[i].tracks, copy).len() == versions[i].tracks.len()).collect();
    match whole.as_slice() {
        // Whole by one: it is named, its files over said as Extra (green: F53.1 (g)).
        [one] => named(State::Agrees, *one, differences(&versions[*one].tracks, copy)),
        // Whole by several, none picked (a pick judges alone): whole, no version named (T-F53-6).
        [_, _, ..] => Release::bare(State::Agrees, copy.len()),
        [] => {
            let counted: Vec<(usize, Vec<Differs>)> = judges.iter().map(|&i| (i, differences(&versions[i].tracks, copy))).collect();
            let fewest = counted.iter().map(|(_, d)| d.len()).min().unwrap_or(0);
            let closest: Vec<&(usize, Vec<Differs>)> = counted.iter().filter(|(_, d)| d.len() == fewest).collect();
            match closest.as_slice() {
                [(i, differ)] => named(State::Closest, *i, differ.clone()),
                _ => Release::bare(State::Closest, copy.len()),
            }
        }
    }
}

/// What the row reads besides the versions (T-F53-2): the finds by name (the arrival's job's and 0.17's carried ones) today's
/// pairing doesn't confirm; whether any sure reference is kept (a reference, or a source's kept words: Last.fm's, OQ-HS01-5 (a));
/// the sources with a sure reference whose list isn't kept yet and that are switched on (Bandcamp's until someone's open reads it,
/// 0109), of those the ones that didn't answer; and the arrival's job's searches not answered yet.
#[derive(Debug, Clone, Default)]
pub struct Kept<'a> {
    pub versions: &'a [ListVersion],
    pub pick: Option<&'a Reference>,
    pub guesses: &'a [Candidate],
    pub referenced: bool,
    pub waiting: &'a [String],
    pub busy: &'a [String],
    pub searching: &'a [String],
}

/// The row (T-F53-2): the verdict where a sure list is kept, with the sources not read yet named beside it; else waiting where a
/// referenced source isn't read yet; no list where references give none; the guesses where no sure reference is kept; waiting on
/// the arrival's searches; else no source knows it, the only red.
pub fn row(copy: &[Present], kept: &Kept) -> Release {
    let mut r = if !kept.versions.is_empty() {
        which_release(copy, kept.versions, kept.pick, &[])
    } else if kept.referenced {
        Release::bare(if kept.waiting.is_empty() { State::NoList } else { State::Waiting }, copy.len())
    } else if !kept.guesses.is_empty() {
        which_release(copy, &[], None, kept.guesses)
    } else if !kept.searching.is_empty() {
        Release::bare(State::Waiting, copy.len())
    } else {
        Release::bare(State::Unknown, copy.len())
    };
    r.waiting = if kept.referenced { kept.waiting.to_vec() } else { kept.searching.to_vec() };
    r.busy = kept.busy.iter().filter(|b| r.waiting.contains(b)).cloned().collect();
    r
}

/// Sources as the row names them, in the domain's order: "MusicBrainz", "MusicBrainz and Bandcamp", "MusicBrainz, Discogs and
/// Bandcamp".
fn named(sources: &[String]) -> String {
    let words: Vec<&str> = sources.iter().map(|s| source_word(s)).collect();
    match words.as_slice() {
        [] => String::new(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// A source's possessive: "MusicBrainz's", "Discogs'".
fn of_source(source: &str) -> String {
    let w = source_word(source);
    if w.ends_with('s') { format!("{w}'") } else { format!("{w}'s") }
}

/// A length as the card shows it: "2:00", "10:38".
pub fn clock(ms: i64) -> String {
    let secs = (ms + 500) / 1000;
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// One difference in words: "track 10's length differs (2:00 against 10:38)", "track 10 is another version (S10 (Live) against
/// S10)", "track 12 (L12) missing", "S11 besides".
fn difference(d: &Differs) -> String {
    let place = |at: &At| match at {
        At::Track { number, .. } => format!("track {number}"),
        At::File(key) => key.rsplit('/').next().unwrap_or(key).to_string(),
    };
    match &d.what {
        What::Length { ours, theirs } => {
            let len = |ms: &Option<i64>| ms.map_or_else(|| "?".to_string(), clock);
            format!("{}'s length differs ({} against {})", place(&d.at), len(ours), len(theirs))
        }
        What::Title { ours, theirs } => format!("{} is another version ({ours} against {theirs})", place(&d.at)),
        What::Missing => format!("{} ({}) missing", place(&d.at), d.title),
        What::Extra => format!("{} besides", d.title),
    }
}

/// The differences in words, up to three, then how many more.
fn differences_said(differ: &[Differs]) -> String {
    let shown: Vec<String> = differ.iter().take(3).map(difference).collect();
    let more = differ.len().saturating_sub(3);
    if more > 0 { format!("{} and {more} more", shown.join("; ")) } else { shown.join("; ") }
}

/// The row's words (the wording table's, F53.11: never the page's). `versions` the ones the row was worked out on; `release_year`
/// a release's year, where kept, to name it where its source keeps several versions (F53.5, T-F53-4).
pub fn said(r: &Release, versions: &[ListVersion], release_year: &dyn Fn(&Reference) -> Option<u32>) -> String {
    let v = r.version.and_then(|i| versions.get(i));
    let who = |v: &ListVersion| -> String {
        // One source keeping several versions: the release is named (F53.5).
        if let ([source], [reference]) = (v.by.as_slice(), v.refs.as_slice())
            && versions.iter().filter(|o| o.by.contains(source)).count() > 1
        {
            let year = release_year(reference).map_or_else(|| reference.value.clone(), |y| y.to_string());
            return format!("{} {year} release", of_source(source));
        }
        named(&v.by)
    };
    let pick = |v: &ListVersion| if r.picked { format!("your pick, {}", who(v)) } else { who(v) };
    let mut out = match (r.state, v) {
        (State::Agrees, Some(v)) => {
            let extra: Vec<&Differs> = r.differ.iter().filter(|d| d.what == What::Extra).collect();
            let besides = if extra.is_empty() { String::new() } else { format!("; {} besides", extra.iter().map(|d| d.title.as_str()).collect::<Vec<_>>().join(", ")) };
            format!("Matches {}: all {} tracks{besides}", pick(v), v.tracks.len())
        }
        (State::Agrees, None) => "Whole by more than one version: see Versions".to_string(),
        (State::Closest, Some(v)) => format!("Closest: {}, {} tracks; {}", pick(v), v.tracks.len(), differences_said(&r.differ)),
        (State::Closest, None) => "Closest to more than one version: see Versions".to_string(),
        (State::NoList, _) => "No source gives its tracklist".to_string(),
        (State::Guessed, _) => {
            let each: Vec<String> = r.guesses.iter().map(|g| format!("{} {}", of_source(&g.reference.source), g.title.clone().unwrap_or_else(|| g.reference.value.clone()))).collect();
            format!("Probably {} (found by its names)", each.join(" or "))
        }
        (State::Waiting, _) if r.waiting.is_empty() => "Being looked for".to_string(),
        (State::Waiting, _) => format!("Waiting for {}", named(&r.waiting)),
        (State::Unknown, _) => "No source knows it".to_string(),
    };
    if r.state != State::Waiting {
        let busy: Vec<String> = r.waiting.iter().filter(|w| r.busy.contains(w)).cloned().collect();
        let unread: Vec<String> = r.waiting.iter().filter(|w| !r.busy.contains(w)).cloned().collect();
        if !busy.is_empty() {
            out.push_str(&format!("; {} didn't answer; asked again later", named(&busy)));
        }
        if !unread.is_empty() {
            out.push_str(&format!("; {} not read yet", named(&unread)));
        }
    }
    out
}

/// A track line's reference length and the sources that say it (F53.4): the named version's track the file pairs with, said by
/// every source of the version ("MusicBrainz, Bandcamp say 10:38"); none where no version is named or the file pairs none.
pub fn track_says(r: &Release, file: usize) -> Option<(i64, String)> {
    let track = r.tracks.get(file)?.as_ref()?;
    let ms = track.length_ms?;
    let by: Vec<&str> = r.by.iter().map(|s| source_word(s)).collect();
    Some((ms, format!("{} {} {}", by.join(", "), if by.len() == 1 { "says" } else { "say" }, clock(ms))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::{List, versions};

    fn t(position: u32, title: &str, ms: Option<i64>) -> RefTrack {
        RefTrack { disc: 1, position, title: title.to_string(), length_ms: ms, recording: None }
    }

    fn f(number: u32, title: &str, ms: i64) -> Present {
        Present { disc: Some(1), number: Some(number), key: format!("IN2/{number:02} {title}.mp3"), title: title.to_string(), named: Vec::new(), ms: Some(ms), told: None }
    }

    fn list(source: &str, kind: &str, value: &str, tracks: Vec<RefTrack>) -> List {
        List { reference: Reference::new(source, kind, value), tracks, at: 1, guessed: false }
    }

    /// S1–S12's lengths: track n 200 s + n s.
    fn ms(n: u32) -> i64 {
        200_000 + i64::from(n) * 1_000
    }

    fn salt(n: u32) -> Vec<RefTrack> {
        (1..=n).map(|i| t(i, &format!("S{i}"), Some(ms(i)))).collect()
    }

    /// Salt Lines (F53's fixtures): MusicBrainz 10 (R70, by its group), Bandcamp 10 (P70, agreeing within 1 s), Discogs 12 (D70).
    fn salt_lines() -> Vec<ListVersion> {
        let bandcamp: Vec<RefTrack> = (1..=10).map(|i| t(i, &format!("S{i}"), Some(ms(i) + 600))).collect();
        versions(&[list("musicbrainz", "release", "R70", salt(10)), list("bandcamp", "album", "P70", bandcamp), list("discogs", "release", "D70", salt(12))])
    }

    fn copy(n: u32) -> Vec<Present> {
        (1..=n).map(|i| f(i, &format!("S{i}"), ms(i))).collect()
    }

    fn sources(r: &Release) -> Vec<&str> {
        r.by.iter().map(String::as_str).collect()
    }

    fn whats(r: &Release) -> Vec<(String, What)> {
        r.differ.iter().map(|d| (d.title.clone(), d.what.clone())).collect()
    }

    /// F53.1: one verdict from every kept list, none ranked. Salt Lines' V10 (MusicBrainz, Bandcamp) and V12 (Discogs), no pick;
    /// copies (a)–(i). Never MusicBrainz alone where Bandcamp agrees; never one of two whole versions named over the other; never a
    /// pair past the slack; never Agrees by a count or by a length alone; never amber for files over; never a version named by the
    /// sources' order or by which was kept first.
    #[test]
    fn which_release_reads_every_version_none_ranked() {
        let v = salt_lines();
        assert_eq!(v.len(), 2, "V10 and V12");
        let v10 = v.iter().position(|x| x.tracks.len() == 10).unwrap();
        let v12 = v.iter().position(|x| x.tracks.len() == 12).unwrap();
        // (a) S1–S10: Agrees with V10, by MusicBrainz and Bandcamp.
        let a = which_release(&copy(10), &v, None, &[]);
        assert_eq!((a.state, a.version, sources(&a)), (State::Agrees, Some(v10), vec!["musicbrainz", "bandcamp"]));
        assert!(a.differ.is_empty());
        // (b) S1–S12: whole by both, none picked: no version named, no source, no difference.
        let b = which_release(&copy(12), &v, None, &[]);
        assert_eq!((b.state, b.version, b.by.len(), b.differ.len()), (State::Agrees, None, 0, 0), "{b:?}");
        // (c) S7 5 s longer than both: Closest V10, track 7 said once as a length; V12 three.
        let mut c = copy(10);
        c[6].ms = Some(ms(7) + 5_000);
        let rc = which_release(&c, &v, None, &[]);
        assert_eq!((rc.state, rc.version), (State::Closest, Some(v10)));
        assert_eq!(whats(&rc), vec![("S7".to_string(), What::Length { ours: Some(ms(7) + 5_000), theirs: Some(ms(7)) })]);
        assert!(rc.tracks[6].is_none(), "track 7 never paired with S7");
        assert_eq!(differences(&v[v12].tracks, &c).len(), 3, "V12: the length and S11, S12 missing");
        // (d) S1–S9: Closest V10, track 10 missing.
        let d = which_release(&copy(9), &v, None, &[]);
        assert_eq!((d.state, d.version, whats(&d)), (State::Closest, Some(v10), vec![("S10".to_string(), What::Missing)]));
        // (e) "S4 (Bonus Track)" is the same title.
        let mut e = copy(10);
        e[3].title = "S4 (Bonus Track)".into();
        assert_eq!((which_release(&e, &v, None, &[]).state, which_release(&e, &v, None, &[]).version), (State::Agrees, Some(v10)));
        // (f) "S10 (Live)" within 1 s: another version, said as a title.
        let mut fl = copy(10);
        fl[9].title = "S10 (Live)".into();
        fl[9].ms = Some(ms(10) + 900);
        let rf = which_release(&fl, &v, None, &[]);
        assert_eq!((rf.state, rf.version), (State::Closest, Some(v10)), "never Agrees by the length alone");
        assert_eq!(whats(&rf), vec![("S10".to_string(), What::Title { ours: "S10 (Live)".into(), theirs: "S10".into() })]);
        // (g) S1–S12 with V10 alone kept: Agrees with V10, S11 and S12 said as extra, green.
        let only = versions(&[list("musicbrainz", "release", "R70", salt(10)), list("bandcamp", "album", "P70", salt(10))]);
        let g = which_release(&copy(12), &only, None, &[]);
        assert_eq!((g.state.colour(), sources(&g)), ("green", vec!["musicbrainz", "bandcamp"]));
        assert_eq!(whats(&g), vec![("S11".to_string(), What::Extra), ("S12".to_string(), What::Extra)]);
        // Low Ceiling: MusicBrainz L1–L9 (V9), Discogs L1–L8, L10–L12 (V11).
        let low = |titles: &[u32]| -> Vec<RefTrack> { titles.iter().enumerate().map(|(i, n)| t(i as u32 + 1, &format!("L{n}"), Some(ms(*n)))).collect() };
        let lc = versions(&[list("musicbrainz", "release", "R72", low(&[1, 2, 3, 4, 5, 6, 7, 8, 9])), list("discogs", "release", "D72", low(&[1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12]))]);
        let v11 = lc.iter().position(|x| x.tracks.len() == 11).unwrap();
        let held = |ns: &[u32]| -> Vec<Present> { ns.iter().enumerate().map(|(i, n)| f(i as u32 + 1, &format!("L{n}"), ms(*n))).collect() };
        // (h) L1–L8, L10, L11: Closest V11, L12 missing (one difference, V9 three).
        let h = which_release(&held(&[1, 2, 3, 4, 5, 6, 7, 8, 10, 11]), &lc, None, &[]);
        assert_eq!((h.state, h.version, whats(&h)), (State::Closest, Some(v11), vec![("L12".to_string(), What::Missing)]));
        // (i) L1–L8, L10: V9 and V11 two each, none picked: Closest, none named, nothing said.
        let i = which_release(&held(&[1, 2, 3, 4, 5, 6, 7, 8, 10]), &lc, None, &[]);
        assert_eq!((i.state, i.version, i.differ.len(), i.by.len()), (State::Closest, None, 0, 0), "{i:?}");
        // The order the lists were kept in names nothing: the versions made the other way round give the same rows.
        let reversed = versions(&[list("discogs", "release", "D70", salt(12)), list("bandcamp", "album", "P70", (1..=10).map(|i| t(i, &format!("S{i}"), Some(ms(i) + 600))).collect()), list("musicbrainz", "release", "R70", salt(10))]);
        for n in [9, 10, 12] {
            let (x, y) = (which_release(&copy(n), &v, None, &[]), which_release(&copy(n), &reversed, None, &[]));
            assert_eq!((x.state, sources(&x), x.differ.len()), (y.state, sources(&y), y.differ.len()), "{n} files");
        }
    }

    /// F53.4 (pure): the closest version names what differs, and each track line its version's sources. (a) Tide Book's file "T10"
    /// (3:11) agrees with Discogs' "T10 (Edit)" (3:10), MusicBrainz's 7:40 not named; (b) "T10 (Edit)" too; (c) Low Ceiling's eleven
    /// agree with Discogs' eleven; (d) S10's file 2:00 against 10:38: Closest V10, one length. Never a track paired within the slack
    /// said as differing; never a crossed pair said as two.
    #[test]
    fn the_closest_version_names_what_differs() {
        let tide_mb: Vec<RefTrack> = (1..=10).map(|i| t(i, &format!("T{i}"), Some(if i == 10 { 460_000 } else { ms(i) }))).collect();
        let tide_dc: Vec<RefTrack> = (1..=10).map(|i| if i == 10 { t(10, "T10 (Edit)", Some(190_000)) } else { t(i, &format!("T{i}"), Some(ms(i))) }).collect();
        let tide = versions(&[list("musicbrainz", "release", "R71", tide_mb), list("discogs", "release", "D71", tide_dc)]);
        let files = |title10: &str| -> Vec<Present> { (1..=10).map(|i| if i == 10 { f(10, title10, 191_000) } else { f(i, &format!("T{i}"), ms(i)) }).collect() };
        for title in ["T10", "T10 (Edit)"] {
            let r = which_release(&files(title), &tide, None, &[]);
            assert_eq!((r.state, sources(&r)), (State::Agrees, vec!["discogs"]), "{title}: {r:?}");
            assert_eq!(track_says(&r, 9).map(|(ms, _)| ms), Some(190_000), "the line says Discogs' length");
            assert!(track_says(&r, 9).unwrap().1.starts_with("Discogs says"), "never MusicBrainz where it is not in the version");
        }
        // (d) S10 2:00 against 10:38 in both lists.
        let long = |n: u32| -> Vec<RefTrack> { (1..=n).map(|i| t(i, &format!("S{i}"), Some(if i == 10 { 638_000 } else { ms(i) }))).collect() };
        let v = versions(&[list("musicbrainz", "release", "R70", long(10)), list("bandcamp", "album", "P70", long(10)), list("discogs", "release", "D70", long(12))]);
        let mut c = copy(10);
        c[9].ms = Some(120_000);
        let r = which_release(&c, &v, None, &[]);
        assert_eq!((r.state, sources(&r)), (State::Closest, vec!["musicbrainz", "bandcamp"]));
        assert_eq!(whats(&r), vec![("S10".to_string(), What::Length { ours: Some(120_000), theirs: Some(638_000) })]);
        let words = said(&r, &v, &|_| None);
        assert_eq!(words, "Closest: MusicBrainz and Bandcamp, 10 tracks; track 10's length differs (2:00 against 10:38)");
        assert_eq!(track_says(&r, 0).unwrap().1, format!("MusicBrainz, Bandcamp say {}", clock(ms(1))));
        assert_eq!(track_says(&r, 9), None, "track 10's file pairs none");
        // A crossed pair (PAIR01.1's shape): two files swapped in place pair by title, and nothing differs.
        let mut crossed = copy(10);
        crossed.swap(2, 3);
        (crossed[2].number, crossed[3].number) = (Some(3), Some(4));
        let only = versions(&[list("musicbrainz", "release", "R70", salt(10))]);
        let x = which_release(&crossed, &only, None, &[]);
        assert_eq!((x.state, x.differ.len()), (State::Agrees, 0), "{x:?}");
        // Where no version is named, no track line names a length.
        let b = which_release(&copy(12), &salt_lines(), None, &[]);
        assert!((0..12).all(|i| track_says(&b, i).is_none()));
    }

    /// F53.5: two releases of one source. Grey Shore's R141a (2013) and R141b (2015, G3 6 s longer), two lists; the copy holds
    /// R141b's G3: Agrees with R141b's version, the release named; where both lists agree, one version, both releases folded, named
    /// as the source. Never R141a named because it was read first.
    #[test]
    fn the_row_names_the_release_it_matches() {
        let grey = |g3: i64| -> Vec<RefTrack> { (1..=10).map(|i| t(i, &format!("G{i}"), Some(if i == 3 { g3 } else { ms(i) }))).collect() };
        let year = |r: &Reference| match r.value.as_str() {
            "R141a" => Some(2013),
            "R141b" => Some(2015),
            _ => None,
        };
        let copy: Vec<Present> = (1..=10).map(|i| f(i, &format!("G{i}"), if i == 3 { ms(3) + 6_000 } else { ms(i) })).collect();
        for order in [["R141a", "R141b"], ["R141b", "R141a"]] {
            let lists: Vec<List> = order.iter().map(|r| list("musicbrainz", "release", r, grey(if *r == "R141a" { ms(3) } else { ms(3) + 6_000 }))).collect();
            let v = versions(&lists);
            let r = which_release(&copy, &v, None, &[]);
            assert_eq!((r.state, r.releases.iter().map(|x| x.value.as_str()).collect::<Vec<_>>()), (State::Agrees, vec!["R141b"]), "{order:?}");
            assert_eq!(said(&r, &v, &year), "Matches MusicBrainz's 2015 release: all 10 tracks");
        }
        let same = versions(&[list("musicbrainz", "release", "R141a", grey(ms(3))), list("musicbrainz", "release", "R141b", grey(ms(3)))]);
        let plain: Vec<Present> = (1..=10).map(|i| f(i, &format!("G{i}"), ms(i))).collect();
        let r = which_release(&plain, &same, None, &[]);
        assert_eq!((r.state, r.releases.len()), (State::Agrees, 2), "one version, both releases folded");
        assert_eq!(said(&r, &same, &year), "Matches MusicBrainz: all 10 tracks");
    }

    /// F53.6: with a pick, the pick judges. Root picked Discogs' twelve: (a) S1–S12 Agrees with the pick; (b) S1–S10 Closest to the
    /// pick, S11 and S12 missing, though V10 holds every one of its tracks. After Reset, (b) Agrees with V10. Never a pick passed over
    /// for a version the copy holds whole; never the pick's words after Reset.
    #[test]
    fn the_pick_judges_the_row() {
        let v = salt_lines();
        let d70 = Reference::new("discogs", "release", "D70");
        let a = which_release(&copy(12), &v, Some(&d70), &[]);
        assert_eq!((a.state, sources(&a), a.picked), (State::Agrees, vec!["discogs"], true));
        assert_eq!(said(&a, &v, &|_| None), "Matches your pick, Discogs: all 12 tracks");
        let b = which_release(&copy(10), &v, Some(&d70), &[]);
        assert_eq!((b.state, sources(&b)), (State::Closest, vec!["discogs"]));
        assert_eq!(whats(&b), vec![("S11".to_string(), What::Missing), ("S12".to_string(), What::Missing)]);
        let reset = which_release(&copy(10), &v, None, &[]);
        assert_eq!((reset.state, sources(&reset), reset.picked), (State::Agrees, vec!["musicbrainz", "bandcamp"], false));
        assert!(!said(&reset, &v, &|_| None).contains("pick"));
    }

    /// T-F53-2's states through `row`: a list kept judges with the sources not read named beside it (a busy one said so); no list
    /// kept and a referenced source not read: waiting; references giving no list: no list; guesses only where no sure reference is
    /// kept; the searches not answered: waiting; nothing at all: the only red.
    #[test]
    fn the_rows_states_follow_what_is_kept() {
        let v = versions(&[list("bandcamp", "album", "P70", salt(10))]);
        let (both, busy) = (vec!["musicbrainz".to_string(), "discogs".to_string()], vec!["musicbrainz".to_string()]);
        let r = row(&copy(10), &Kept { versions: &v, referenced: true, waiting: &both, busy: &busy, ..Default::default() });
        assert_eq!((r.state, sources(&r)), (State::Agrees, vec!["bandcamp"]));
        assert_eq!(said(&r, &v, &|_| None), "Matches Bandcamp: all 10 tracks; MusicBrainz didn't answer; asked again later; Discogs not read yet");
        let w = row(&copy(10), &Kept { referenced: true, waiting: &both, ..Default::default() });
        assert_eq!(w.state, State::Waiting);
        assert_eq!(row(&copy(10), &Kept { referenced: true, ..Default::default() }).state, State::NoList);
        let guess = [Candidate { reference: Reference::new("musicbrainz", "release", "R143"), title: Some("Untitled Shore".into()) }];
        let g = row(&copy(10), &Kept { guesses: &guess, searching: &busy, ..Default::default() });
        assert_eq!((g.state, g.guesses.len()), (State::Guessed, 1));
        assert!(said(&g, &[], &|_| None).starts_with("Probably MusicBrainz's Untitled Shore"));
        assert_eq!(row(&copy(10), &Kept { searching: &both, ..Default::default() }).state, State::Waiting);
        assert_eq!(row(&copy(10), &Kept::default()).state.colour(), "red");
    }
}
