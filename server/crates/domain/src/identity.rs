//! Whether two names are one (DOMAIN-CORE §12, the one identity matcher's first part): the one title rule, X3's, for a file
//! against a list's track. Soulseek's folders and the holding's copies pair by it (PAIR01, OQ-PAIR01-1 (a), the owner
//! 2026-11-20); `assign` owns the lengths, the tiers and the ties, and is handed this rule's verdict.

use cratarr_rules::names::loose;

/// Words that make a recording another version of its title (X3): a file's title saying one its list's title does not is not
/// that track.
pub const VERSIONS: [&str; 12] = ["instrumental", "live", "remix", "mix", "acoustic", "demo", "karaoke", "acapella", "cappella", "unplugged", "reprise", "edit"];

/// A title as compared: loose, its "(Remastered)" or "- Live" left off too.
fn titled(t: &str) -> [String; 2] {
    let base = t.split(" (").next().unwrap_or(t).split(" [").next().unwrap_or(t).split(" - ").next().unwrap_or(t);
    [loose(t), loose(base)]
}

/// Two titles of one track, loosely: the same loose name, or the same before a bracket or " - ".
pub fn same_title(a: &str, b: &str) -> bool {
    let (x, y) = (titled(a), titled(b));
    !x[0].is_empty() && (x[0] == y[0] || (!x[1].is_empty() && x[1] == y[1]))
}

/// Whether a file's title is a list track's (X3, the one title rule): as [`same_title`] has it, but an "Artist - Title" list
/// compares its title part, never its artist alone; and a version word in brackets or after " - " that the list's title lacks
/// (Instrumental, Live, Remix…) makes the file another version. A word on the list's side only still pairs (OQ-PAIR01-2 (a)),
/// and a bracket that names no version ("(Bonus Track)", "(Remastered)") is the same title (OQ-PAIR01-1 (a)).
pub fn titled_as(file: &str, list: &str) -> bool {
    let words = |s: &str| -> Vec<String> { s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_string).collect() };
    let listed = words(list);
    let lower = file.to_lowercase();
    let marked: Vec<&str> = lower.split(['(', '[']).skip(1).chain(lower.split(" - ").skip(1)).collect();
    if marked.iter().any(|part| words(part).iter().any(|w| VERSIONS.contains(&w.as_str()) && !listed.contains(w))) {
        return false;
    }
    match list.split_once(" - ") {
        Some((_, title)) => loose(file) == loose(list) || same_title(file, title.trim()),
        None => same_title(file, list),
    }
}

/// What an arrival is judged by (DOMAIN-CORE §12; SCENARIOS-WM WM.8): the record its placement names (the catalogue's placement row,
/// COU.18: provenance), the MusicBrainz release its files name, its artist (folded; empty where its files name none), and its files
/// (`facts::Present`, each its title through the one tag rule, else its name's, WM.25). Its album's name is no evidence: a stamp is
/// none, and a name alone never joins (X8).
#[derive(Debug, Clone, Default)]
pub struct Arrival<'a> {
    pub placed_for: Option<i64>,
    pub release: Option<&'a str>,
    pub artist: &'a str,
    pub files: &'a [crate::facts::Present],
}

/// A record an arrival may be: its id, the MusicBrainz releases it holds, its artist (folded), the lists it keeps that judge (no
/// guess), and its copies' files.
#[derive(Debug, Clone, Default)]
pub struct Candidate<'a> {
    pub id: i64,
    pub releases: &'a [String],
    pub artist: &'a str,
    pub lists: &'a [Vec<crate::facts::RefTrack>],
    pub copies: &'a [Vec<crate::facts::Present>],
}

/// Why a verdict holds, strongest first (WM.8): the placement's record (provenance); the MusicBrainz release; the titles and lengths
/// (`assign`, the one title rule); the artist agreeing, which the titles need where nothing stronger speaks (T-WM-4); the folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    Provenance(i64),
    MbIds(String),
    Tracks,
    Artist,
    Folder,
}

/// The verdict on an arrival against one record: its copy; another record linked to it as an edition (an arrival fetched for it
/// whose tracks differ from a copy already whole: one card, H6, VR36; or naming its release where the tracks don't pair, FA19); or
/// apart, a record of its own by its folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Same(Vec<Reason>),
    Edition(Vec<Reason>),
    Apart(Vec<Reason>),
}

/// A copy's files as a list to pair against (its files' places, titles and lengths).
pub fn as_list(files: &[crate::facts::Present]) -> Vec<crate::facts::RefTrack> {
    files
        .iter()
        .zip(1..)
        .map(|(f, n)| crate::facts::RefTrack {
            disc: f.disc.unwrap_or(1),
            position: f.number.unwrap_or(n),
            title: if f.title.is_empty() { f.named.first().cloned().unwrap_or_default() } else { f.title.clone() },
            length_ms: f.ms,
            recording: None,
        })
        .collect()
}

/// Whether every one of the arrival's files pairs with a track of `target` (`facts::pairs_with`); by a title each where `titled`
/// (without a placement or ids, pairs by length alone join nothing: T-WM-7), and one for one both ways where `both`.
fn covers(target: &[crate::facts::RefTrack], files: &[crate::facts::Present], titled: bool, both: bool) -> bool {
    let pairs = crate::facts::pairs_with(target, files);
    !files.is_empty()
        && pairs.len() == files.len()
        && (!both || pairs.len() == target.len())
        && (!titled || pairs.iter().all(|&(t, f)| crate::facts::by_title(&target[t], &files[f]) || files[f].told.is_some()))
}

/// The one judgement of an arrival against a record (WM.8; DOMAIN-CORE §12, D.3), the evidence read strongest first, a stronger one
/// saying "not the same" outweighing every weaker:
/// - **its placement** names this record: its copy where every one of its files pairs with a list the record keeps, or a copy's
///   tracks, and no copy of the record is already whole by a list that differs from that one (WM.10, WM.11, OQ-WM-2 (a)); else an
///   edition of it (WM.12). Nothing to pair against: its copy all the same (a record not had, fetched by its names). A placement
///   naming another record says this one is not it, whatever its ids or tracks (WM.8 (f));
/// - **its files' release** is one the record holds: its copy where its files pair; else an edition (FA19);
/// - **its titles and lengths with its artist**: every file and every track of a kept list or a copy one for one, by titles (T-WM-4,
///   T-WM-7);
/// - else **the folder**: a record of its own.
pub fn judge(a: &Arrival, r: &Candidate) -> Verdict {
    let lists: Vec<Vec<crate::facts::RefTrack>> = r.lists.iter().cloned().chain(r.copies.iter().map(|c| as_list(c))).collect();
    if let Some(placed) = a.placed_for {
        if placed != r.id {
            return Verdict::Apart(vec![Reason::Provenance(placed)]);
        }
        if lists.iter().all(Vec::is_empty) {
            return Verdict::Same(vec![Reason::Provenance(placed)]);
        }
        // The lists the arrival is whole by, or a part of: every one of its files pairs with one.
        let by: Vec<&Vec<crate::facts::RefTrack>> = lists.iter().filter(|l| covers(l, a.files, false, false)).collect();
        // A copy of the record already whole by a kept list that differs from every one the arrival pairs with.
        let whole_apart = r.copies.iter().any(|copy| {
            r.lists.iter().any(|m| !m.is_empty() && crate::facts::pairs_with(m, copy).len() == m.len() && !by.iter().any(|l| crate::facts::agree(l, m)))
        });
        return if !by.is_empty() && !whole_apart {
            Verdict::Same(vec![Reason::Provenance(placed), Reason::Tracks])
        } else {
            Verdict::Edition(vec![Reason::Provenance(placed)])
        };
    }
    if let Some(release) = a.release.filter(|v| r.releases.iter().any(|h| h.eq_ignore_ascii_case(v))) {
        let release = release.to_lowercase();
        return if lists.iter().any(|l| covers(l, a.files, false, false)) {
            Verdict::Same(vec![Reason::MbIds(release), Reason::Tracks])
        } else {
            Verdict::Edition(vec![Reason::MbIds(release)])
        };
    }
    if !a.artist.is_empty() && a.artist == r.artist && lists.iter().any(|l| covers(l, a.files, true, true)) {
        return Verdict::Same(vec![Reason::Tracks, Reason::Artist]);
    }
    Verdict::Apart(vec![Reason::Folder])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WM.8: the evidence, strongest first, each verdict with its reasons in that order. Red first: no `judge`.
    #[test]
    fn the_evidence_is_read_strongest_first() {
        use crate::facts::{Present, RefTrack};
        let list = |titles: &[(&str, i64)]| -> Vec<RefTrack> {
            titles.iter().zip(1..).map(|((t, ms), position)| RefTrack { disc: 1, position, title: t.to_string(), length_ms: Some(*ms), recording: None }).collect()
        };
        let files = |titles: &[(&str, i64)]| -> Vec<Present> {
            titles.iter().map(|(t, ms)| Present { key: format!("{t}.mp3"), title: t.to_string(), ms: Some(ms + 400), ..Present::default() }).collect()
        };
        let names = |n: std::ops::RangeInclusive<i64>, word: &str, from: i64, step: i64| -> Vec<(String, i64)> { n.map(|i| (format!("{word}{i}"), from + i * step)).collect() };
        fn refs(v: &[(String, i64)]) -> Vec<(&str, i64)> {
            v.iter().map(|(t, ms)| (t.as_str(), *ms)).collect()
        }
        let mumia = [("T1", 190_000), ("T2", 260_000), ("T3", 330_000), ("T4", 400_000)];
        let (m_list, m_copy) = (vec![list(&mumia)], vec![files(&mumia)]);
        let al3211 = Candidate { id: 3211, artist: "polopan", lists: &m_list, copies: &m_copy, ..Candidate::default() };
        let arrival = files(&mumia);
        // (a) its placement names al-3211, its tracks pair one for one with its copy; (e) another record's album name changes nothing:
        // no name is evidence, so it is no input.
        let a = Arrival { placed_for: Some(3211), artist: "redaxespolopan", files: &arrival, ..Arrival::default() };
        assert_eq!(judge(&a, &al3211), Verdict::Same(vec![Reason::Provenance(3211), Reason::Tracks]));
        // (f) its files name al-156's release: below the placement; against al-156, not the same.
        let r156 = vec!["r156".to_string()];
        let remixes = vec![list(&[("Remix 1", 300_000), ("Remix 2", 310_000)])];
        let al156 = Candidate { id: 156, releases: &r156, artist: "polopan", lists: &remixes, ..Candidate::default() };
        let f = Arrival { release: Some("R156"), ..a.clone() };
        assert_eq!(judge(&f, &al3211), Verdict::Same(vec![Reason::Provenance(3211), Reason::Tracks]));
        assert_eq!(judge(&f, &al156), Verdict::Apart(vec![Reason::Provenance(3211)]), "a placement naming another record outweighs the ids");
        // (b) no placement; its files name R153, which al-153 holds.
        let glass = [("Q1", 200_000), ("Q2", 230_000)];
        let (r153, g_list) = (vec!["r153".to_string()], vec![list(&glass)]);
        let al153 = Candidate { id: 153, releases: &r153, artist: "glassharbour", lists: &g_list, ..Candidate::default() };
        let g_files = files(&glass);
        let b = Arrival { release: Some("r153"), files: &g_files, ..Arrival::default() };
        assert_eq!(judge(&b, &al153), Verdict::Same(vec![Reason::MbIds("r153".into()), Reason::Tracks]));
        // (c) no placement, no ids: its artist agrees and its six tracks agree with al-154's list.
        let shore = names(1..=6, "N", 180_000, 7_000);
        let s_list = vec![list(&refs(&shore))];
        let al154 = Candidate { id: 154, artist: "glassharbour", lists: &s_list, ..Candidate::default() };
        let s_files = files(&refs(&shore));
        let c = Arrival { artist: "glassharbour", files: &s_files, ..Arrival::default() };
        assert_eq!(judge(&c, &al154), Verdict::Same(vec![Reason::Tracks, Reason::Artist]));
        assert_eq!(judge(&Arrival { artist: "kitetheory", ..c.clone() }, &al154), Verdict::Apart(vec![Reason::Folder]), "never without the artist (T-WM-4)");
        let untitled: Vec<Present> = s_files.iter().map(|p| Present { title: String::new(), ..p.clone() }).collect();
        assert_eq!(judge(&Arrival { files: &untitled, ..c.clone() }, &al154), Verdict::Apart(vec![Reason::Folder]), "never by lengths alone (T-WM-7)");
        // (d) no placement, no ids, tracks that pair with no list it keeps: a record of its own, by its folder.
        let other = files(&[("Mumia II 1", 100_000), ("Mumia II 2", 120_000), ("Mumia II 3", 140_000), ("Mumia II 4", 160_000)]);
        assert_eq!(judge(&Arrival { artist: "glassharbour", files: &other, ..Arrival::default() }, &al154), Verdict::Apart(vec![Reason::Folder]));
        // WM.11's rule: partly there, the arrival whole by the list kept: its copy. WM.12's: a copy already whole by a list that
        // differs from the arrival's: an edition.
        let tide = names(1..=10, "W", 150_000, 9_000);
        let (t_list, t_copy) = (vec![list(&refs(&tide))], vec![files(&refs(&tide)[..7])]);
        let al151 = Candidate { id: 151, artist: "kitetheory", lists: &t_list, copies: &t_copy, ..Candidate::default() };
        let t_files = files(&refs(&tide));
        assert_eq!(judge(&Arrival { placed_for: Some(151), files: &t_files, ..Arrival::default() }, &al151), Verdict::Same(vec![Reason::Provenance(151), Reason::Tracks]));
        let pier = names(1..=12, "P", 150_000, 9_000);
        let (p_lists, p_copy) = (vec![list(&refs(&pier)[..10]), list(&refs(&pier))], vec![files(&refs(&pier)[..10])]);
        let al152 = Candidate { id: 152, artist: "kitetheory", lists: &p_lists, copies: &p_copy, ..Candidate::default() };
        let p_files = files(&refs(&pier));
        assert_eq!(judge(&Arrival { placed_for: Some(152), files: &p_files, ..Arrival::default() }, &al152), Verdict::Edition(vec![Reason::Provenance(152)]));
    }

    /// X3 and OQ-PAIR01-1/-2 (a), the owner 2026-11-20: the rule Soulseek's `titled_as` applied before it moved here, its
    /// verdicts kept (requests' `a_bracketed_version_word_is_another_version` pins the caller's side).
    #[test]
    fn one_title_rule_for_files_and_lists() {
        assert!(!titled_as("01 Pier (Instrumental)", "Pier") && !titled_as("Pier - Live", "Pier") && !titled_as("Pier [Remix]", "Pier"));
        assert!(titled_as("Pier (Live)", "Pier (Live)") && titled_as("Pier (Remastered)", "Pier"));
        assert!(titled_as("Pier (Bonus Track)", "Pier"), "a bracket that names no version: the same title");
        assert!(titled_as("Pier", "Pier (Live)"), "a version word on the list's side only still pairs");
        assert!(titled_as("Pier", "Glass Harbour - Pier") && !titled_as("Glass Harbour - Landing", "Glass Harbour - Pier"));
        assert!(!titled_as("Glass Harbour", "Glass Harbour - Pier"), "never by the artist alone");
        assert!(titled_as("Embrace (feat. Phoebe Killdeer)", "Embrace"), "a guest folds");
        assert!(!titled_as("Breakwater", "Pier"));
        assert!(same_title("The Wall (Remastered)", "Wall") && !same_title("", ""));
    }
}
