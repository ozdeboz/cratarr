//! The holding, the one had rule (DOMAIN-CORE §4.3, C.3; SCENARIOS-DC1 HD1-HD15): what a record is to a viewer, worked out
//! from its copies and never stored. It lives in the rules so that the device's index says what the server says (HD12, §14).
//!
//! - **What it reads** ([`CopyFacts`]): each copy's library, how many of its tracks have a present file, how many of those
//!   are paired one for one with the shortest list kept (the store pairs them with [`crate::assign`]; here they are only
//!   read), and when its last file went. Present files only (A3.1): a gone, parked or trashed file is no copy.
//! - **Whole** is tracks paired with the shortest list, never a count of files (HD2); with no list kept, any present copy is
//!   whole (OQ-VR-6 (c), the owner 2026-11-20). **Partly** is the fullest copy, never two halves together (HD3).
//! - **The viewer's view** (I05): whole in a main library or their own music is Had; whole only in someone else's own music,
//!   Elsewhere; only in Incoming, Arrived; a kept copy elsewhere comes before an arrival (OQ-DC1-1 (a)). An upload waiting in
//!   Incoming is no one's but its uploader's until it is added (H3, UP03).
//! - **The place's libraries** ([`On`]): a place counts what its opener does not send as switched off (HD7); the requests
//!   count every library, whatever a viewer's switch says.
//! - **Had for the house** ([`house_has`]) is a rule of its own (HD11): what stops Soulseek and Lidarr.

use serde::{Deserialize, Serialize};

/// Who brought an Incoming copy: Lidarr's, Soulseek's, or an account's upload (the Uploads library, UP01-UP03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fetcher {
    Lidarr,
    Soulseek,
    Uploads,
}

/// What a copy's library is to the holding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Library {
    /// The household's music: every main library alike.
    Main,
    /// A fetcher's arrivals, waiting to be added. `uploader` names whose upload it is, while it waits (UP03).
    Incoming { fetcher: Fetcher, uploader: Option<String> },
    /// An account's own music, by its owner.
    Own(String),
}

/// When a copy's last present file went (gone, deleted or trashed: never the pass that retired its tracks later), and the
/// library it left, by its catalogue folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Went {
    pub at: i64,
    pub from: i64,
}

/// One copy of a record as the holding reads it: the store's `copies_of` makes these, by the present rule (HD8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyFacts {
    /// Its catalogue folder: the library a place's set names.
    pub folder: i64,
    pub library: Library,
    /// Its tracks with a present file.
    pub present: u16,
    /// Of those, the ones paired one for one with the shortest list kept; `present` where the record keeps no list.
    pub paired: u16,
    /// Where none is present any more: when its last went, and from where.
    pub went: Option<Went>,
}

/// What a record is to a viewer, from where they see it. Worked out, never stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum Holding {
    /// No present copy the viewer counts, and none went: never had, or parked (its files come back with the library).
    NotHad,
    /// No copy whole: the fullest one's paired tracks, of the shortest list.
    Partly { has: u16, of: u16 },
    /// Whole only in Incoming, waiting to be added.
    Arrived,
    /// Whole in a main library or the viewer's own music.
    Had,
    /// Whole only in someone else's own music.
    Elsewhere,
    /// No present copy, though one was: when its last file went and the library it left (OQ-DC1-2 (a)).
    Retired { since: i64, from: i64 },
    /// Present copies, but the only lists kept are guesses' (VR12 rule 6, C16, the owner 2026-11-21): not known whole. Never Had,
    /// Arrived, Elsewhere or Partly, and no count ("of") until an administrator picks or confirms a list.
    Unknown,
}

/// What a record keeps to judge its copies whole by (VR12 rules 1, 5, 6): no list (any present copy is whole, OQ-VR-6 (c)); the
/// shortest list that judges, in tracks; or only guesses' lists, which judge nothing (not known whole).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kept {
    NoList,
    Shortest(u16),
    OnlyGuesses,
}

impl Kept {
    /// The shortest list kept, or none: the lists' input as `holding()` takes it.
    pub fn of(shortest: Option<u16>) -> Self {
        shortest.map_or(Kept::NoList, Kept::Shortest)
    }
}

/// The libraries a place counts (HD7): every one but those its opener sends as switched off.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct On {
    pub without: Vec<i64>,
}

impl On {
    /// Every library: the requests' set, and every place that doesn't follow the switch.
    pub fn all() -> Self {
        Self::default()
    }

    /// Every library but these catalogue folders.
    pub fn without(folders: &[i64]) -> Self {
        Self { without: folders.to_vec() }
    }

    /// Whether a library counts here.
    pub fn counts(&self, folder: i64) -> bool {
        !self.without.contains(&folder)
    }
}

/// A copy the viewer may count: any but an upload waiting for someone else (H3, UP03).
fn seen_by(copy: &CopyFacts, viewer: &str) -> bool {
    match &copy.library {
        Library::Incoming { uploader: Some(uploader), .. } => uploader.as_str() == viewer,
        _ => true,
    }
}

/// Whole: every track of the shortest list paired; with no list kept, present at all. The one whole rule: a caller that picks a
/// whole copy (a request's asker's copy, RQ01.6) reads it here.
pub fn whole(copy: &CopyFacts, shortest: Option<u16>) -> bool {
    copy.present > 0 && shortest.is_none_or(|of| copy.paired >= of)
}

/// Whole, the lists kept said whole (`kept`): never by a guess (VR12 rule 6).
pub fn whole_of(copy: &CopyFacts, kept: Kept) -> bool {
    match kept {
        Kept::OnlyGuesses => false,
        Kept::NoList => whole(copy, None),
        Kept::Shortest(of) => whole(copy, Some(of)),
    }
}

/// The holding (HD1's rule, in order): what `viewer` (an account) has of a record with these `copies`, the shortest list
/// kept `shortest` tracks long (none where it keeps no list), from a place that counts the libraries `on`.
pub fn holding(copies: &[CopyFacts], shortest: Option<u16>, viewer: &str, on: &On) -> Holding {
    holding_of(copies, Kept::of(shortest), viewer, on)
}

/// The holding as [`holding`] has it, the lists kept said whole (`kept`): where only guesses are kept, a present copy the viewer
/// counts is not known whole ([`Holding::Unknown`], VR12 rule 6), never whole by them and never counted against them.
pub fn holding_of(copies: &[CopyFacts], kept: Kept, viewer: &str, on: &On) -> Holding {
    let counted: Vec<&CopyFacts> = copies.iter().filter(|c| c.present > 0 && on.counts(c.folder) && seen_by(c, viewer)).collect();
    if kept == Kept::OnlyGuesses && !counted.is_empty() {
        return Holding::Unknown;
    }
    let shortest = match kept {
        Kept::Shortest(of) => Some(of),
        Kept::NoList | Kept::OnlyGuesses => None,
    };
    let wholly_in: Vec<&Library> = counted.iter().filter(|c| whole(c, shortest)).map(|c| &c.library).collect();
    let theirs = |library: &Library| match library {
        Library::Main => true,
        Library::Own(owner) => owner.as_str() == viewer,
        Library::Incoming { .. } => false,
    };
    if wholly_in.iter().any(|&library| theirs(library)) {
        return Holding::Had;
    }
    // Own music, and not theirs (that was Had): someone else's.
    if wholly_in.iter().any(|&library| matches!(library, Library::Own(_))) {
        return Holding::Elsewhere;
    }
    if wholly_in.iter().any(|&library| matches!(library, Library::Incoming { .. })) {
        return Holding::Arrived;
    }
    if let (Some(of), Some(has)) = (shortest, counted.iter().map(|c| c.paired).max()) {
        return Holding::Partly { has, of };
    }
    // Retired only where no copy is present anywhere: a copy in a library switched off, or an upload waiting, is still there.
    if copies.iter().all(|c| c.present == 0)
        && let Some(went) = copies.iter().filter_map(|c| c.went).max_by_key(|w| w.at)
    {
        return Holding::Retired { since: went.at, from: went.from };
    }
    Holding::NotHad
}

/// Had for the house (HD11): a copy whole in any library but a waiting upload. Soulseek's RecordHad and the round's copy for
/// an asker read it; whether the asker can get that copy (their own music on, their room) is the request machine's (DC2).
pub fn house_has(copies: &[CopyFacts], shortest: Option<u16>) -> bool {
    copies.iter().any(|c| whole(c, shortest) && !matches!(c.library, Library::Incoming { uploader: Some(_), .. }))
}

/// Had for the house, the lists kept said whole (`kept`): only guesses kept, no copy is whole (VR12 rule 6).
pub fn house_has_of(copies: &[CopyFacts], kept: Kept) -> bool {
    match kept {
        Kept::OnlyGuesses => false,
        Kept::NoList => house_has(copies, None),
        Kept::Shortest(of) => house_has(copies, Some(of)),
    }
}

/// A state line: its words, and the time the page writes with its one `when()` (SB13).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub words: String,
    pub at: Option<i64>,
}

/// What a retired record says (HD9, OQ-DC1-2 (a)): the library its last copy left, by its name, and the day that copy went,
/// never the pass's day. The other holdings' lines are the wording table's (RS2).
pub fn was_in(library: &str, since: i64) -> Line {
    Line { words: format!("Was in {library} until"), at: Some(since) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assign::{Fit, Side, assign, fit};

    // The libraries of SCENARIOS-DC1, by their catalogue folders.
    const MUSIC: i64 = 0;
    const IN: i64 = 1;
    const OWN_A: i64 = 2;
    const OWN_B: i64 = 3;
    const MORE: i64 = 4;
    const UPL: i64 = 6;
    /// 2026-03-03 12:00 UTC: when Night Ferry's last copy went (HD9).
    const T: i64 = 1_772_539_200;

    fn copy(folder: i64, library: Library, paired: u16) -> CopyFacts {
        CopyFacts { folder, library, present: paired, paired, went: None }
    }

    fn main_copy(folder: i64, paired: u16) -> CopyFacts {
        copy(folder, Library::Main, paired)
    }

    fn incoming(folder: i64, paired: u16) -> CopyFacts {
        copy(folder, Library::Incoming { fetcher: Fetcher::Lidarr, uploader: None }, paired)
    }

    fn own(folder: i64, owner: &str, paired: u16) -> CopyFacts {
        copy(folder, Library::Own(owner.to_string()), paired)
    }

    fn upload(by: &str, paired: u16) -> CopyFacts {
        copy(UPL, Library::Incoming { fetcher: Fetcher::Uploads, uploader: Some(by.to_string()) }, paired)
    }

    fn gone(folder: i64, at: i64) -> CopyFacts {
        CopyFacts { folder, library: Library::Main, present: 0, paired: 0, went: Some(Went { at, from: folder }) }
    }

    fn of_ten(copies: &[CopyFacts], viewer: &str) -> Holding {
        holding(copies, Some(10), viewer, &On::all())
    }

    /// HD1. Red first against today's `holding_of` (app/requests.rs:2038, the requests crate after the move night) on the
    /// rows HD10 names: it counts present tracks across copies, not the fullest copy's pairs.
    #[test]
    fn the_holding_rule_in_order() {
        let rows: [(&str, Vec<CopyFacts>, Holding); 10] = [
            ("(a) MUSIC 10", vec![main_copy(MUSIC, 10)], Holding::Had),
            ("(b) OWN-A 10", vec![own(OWN_A, "alice", 10)], Holding::Had),
            ("(c) IN 10", vec![incoming(IN, 10)], Holding::Arrived),
            ("(d) OWN-B 10", vec![own(OWN_B, "bob", 10)], Holding::Elsewhere),
            ("(e) MUSIC 7", vec![main_copy(MUSIC, 7)], Holding::Partly { has: 7, of: 10 }),
            ("(f) MUSIC 7 and IN 6", vec![main_copy(MUSIC, 7), incoming(IN, 6)], Holding::Partly { has: 7, of: 10 }),
            ("(g) none present, the last gone at t from MORE", vec![gone(MORE, T)], Holding::Retired { since: T, from: MORE }),
            ("(h) none, ever", vec![], Holding::NotHad),
            ("(i) IN 10 and MUSIC 7", vec![incoming(IN, 10), main_copy(MUSIC, 7)], Holding::Arrived),
            ("(j) MUSIC 10 and IN 10", vec![main_copy(MUSIC, 10), incoming(IN, 10)], Holding::Had),
        ];
        for (row, copies, expected) in rows {
            assert_eq!(of_ten(&copies, "alice"), expected, "{row}");
        }
        // Never a holding stored: it is this function of its copies alone, the same each time it is asked.
        assert_eq!(of_ten(&[main_copy(MUSIC, 7)], "alice"), of_ten(&[main_copy(MUSIC, 7)], "alice"));
    }

    /// HD2. Red first against `holding_of`'s `n >= l` (app/requests.rs:2016): the second copy reads whole by its ten files.
    /// The pairs come from `assign` here as the store's `copies_of` makes them (PAIR01.18 is the store's half).
    #[test]
    fn whole_by_tracks_paired_with_the_shortest_list() {
        let list: Vec<(String, i64)> = (1..=10).map(|n| (format!("A{n}"), 200_000 + n * 1_000)).collect();
        let paired = |files: &[(String, i64)]| -> u16 {
            let tracks: Vec<Side> = list.iter().map(|(title, ms)| Side { disc: None, number: None, key: title, length_ms: Some(*ms) }).collect();
            let sides: Vec<Side> = files.iter().map(|(title, ms)| Side { disc: None, number: None, key: title, length_ms: Some(*ms) }).collect();
            let pairs = assign(&tracks, &sides, |t, f| if list[t].0 == files[f].0 { fit(Some(list[t].1), Some(files[f].1)) } else { Fit::No });
            u16::try_from(pairs.len()).unwrap()
        };
        let present = |files: &[(String, i64)], paired: u16| CopyFacts { folder: MUSIC, library: Library::Main, present: files.len() as u16, paired, went: None };
        // A1-A10.
        let whole = list.clone();
        // A1-A8 and two tracks no list has (ten files).
        let mut short: Vec<(String, i64)> = list[..8].to_vec();
        short.extend([("Breakwater".to_string(), 90_000), ("Tide".to_string(), 95_000)]);
        // Twelve files, A1-A10 among them.
        let mut twelve = list.clone();
        twelve.extend([("Bonus 1".to_string(), 100_000), ("Bonus 2".to_string(), 110_000)]);
        assert_eq!(of_ten(&[present(&whole, paired(&whole))], "alice"), Holding::Had);
        let short_copy = present(&short, paired(&short));
        assert_eq!((short_copy.present, short_copy.paired), (10, 8));
        assert_eq!(of_ten(&[short_copy], "alice"), Holding::Partly { has: 8, of: 10 }, "never whole by its count of ten files");
        assert_eq!(of_ten(&[present(&twelve, paired(&twelve))], "alice"), Holding::Had);
        // A record that keeps no list: its copy of nine files, `paired` = `present`, whole (OQ-VR-6 (c)).
        let nine = CopyFacts { folder: MUSIC, library: Library::Main, present: 9, paired: 9, went: None };
        assert_eq!(holding(&[nine], None, "alice", &On::all()), Holding::Had);
    }

    /// HD3. Red first: `holding_of` gives Partly { 10, 10 }, counting present tracks across copies (a named row of HD10).
    #[test]
    fn partly_is_the_fullest_copy() {
        // MUSIC holds tracks 1-6, IN holds 5-10: each paired alone, 6 and 6.
        let halves = [main_copy(MUSIC, 6), incoming(IN, 6)];
        assert_eq!(of_ten(&halves, "alice"), Holding::Partly { has: 6, of: 10 });
        assert_ne!(of_ten(&halves, "alice"), Holding::Partly { has: 10, of: 10 });
        assert_ne!(of_ten(&halves, "alice"), Holding::Had, "never Had from two halves");
    }

    /// HD4 (I05). A guard: green today through `records_held`'s `in_view` (app/requests.rs:1938), which the one rule keeps.
    #[test]
    fn the_holding_is_the_viewers() {
        let only_b = [own(OWN_B, "bob", 10)];
        assert_eq!(of_ten(&only_b, "bob"), Holding::Had);
        // Alice, root, and the reading key (root's).
        for viewer in ["alice", "root"] {
            assert_eq!(of_ten(&only_b, viewer), Holding::Elsewhere, "{viewer}: never Had on bob's copy");
        }
        let with_music = [own(OWN_B, "bob", 10), main_copy(MUSIC, 10)];
        for viewer in ["alice", "bob", "root"] {
            assert_eq!(of_ten(&with_music, viewer), Holding::Had, "{viewer}");
        }
    }

    /// HD5 (OQ-DC1-1 (a), as today: the owner's rules, 2026-11-19). A guard: `records_held` turns Done into Elsewhere today.
    #[test]
    fn a_kept_copy_elsewhere_comes_before_an_arrival() {
        let copies = [incoming(IN, 10), own(OWN_B, "bob", 10)];
        assert_eq!(of_ten(&copies, "alice"), Holding::Elsewhere, "never Arrived while bob's own music holds it whole");
        assert_eq!(of_ten(&copies, "bob"), Holding::Had);
    }

    /// HD6 (H3, UP03). Red first against today's had rule, for which a waiting upload is Arrived for every account's request.
    #[test]
    fn an_upload_is_no_one_elses_arrival() {
        let waiting = [upload("bob", 10)];
        assert_eq!(of_ten(&waiting, "bob"), Holding::Arrived, "his upload waits for him to add it");
        assert_eq!(of_ten(&waiting, "alice"), Holding::NotHad, "as if the upload were not there: her request stays open");
        assert_eq!(of_ten(&waiting, "root"), Holding::NotHad);
        assert!(!house_has(&waiting, Some(10)), "Soulseek's item stays, Lidarr is told nothing");
        // Once bob adds it to MUSIC: Had for all.
        let added = [main_copy(MUSIC, 10)];
        for viewer in ["alice", "bob", "root"] {
            assert_eq!(of_ten(&added, viewer), Holding::Had, "{viewer}");
        }
        // Never Arrived for anyone but the uploader, partly there or whole.
        assert_eq!(of_ten(&[upload("bob", 4)], "alice"), Holding::NotHad);
    }

    /// HD9 (OQ-DC1-2 (a)). Red first against today's "Was in Music until" with `retired_at` (recordNotHad.ts:30): the pass's
    /// day, and "Music" whichever library held it.
    #[test]
    fn retired_says_its_library_and_when_its_last_copy_went() {
        // Night Ferry's one copy, in MORE, deleted outside Cratarr at t; the pass retires its tracks at t + 25 h.
        let retired = of_ten(&[gone(MORE, T)], "alice");
        assert_eq!(retired, Holding::Retired { since: T, from: MORE });
        assert_ne!(retired, Holding::Retired { since: T + 25 * 3_600, from: MORE }, "never the pass's moment as the day");
        let line = was_in("MORE", T);
        assert_eq!(line, Line { words: "Was in MORE until".to_string(), at: Some(T) });
        assert!(!line.words.contains("Music"), "never Music for another library");
        // The latest copy to go names the library; a copy still present anywhere is no retirement.
        assert_eq!(of_ten(&[gone(MUSIC, T - 100), gone(MORE, T)], "alice"), Holding::Retired { since: T, from: MORE });
        assert_eq!(holding(&[gone(MORE, T), main_copy(MUSIC, 10)], Some(10), "alice", &On::without(&[MUSIC])), Holding::NotHad);
    }

    /// HD11 (kind 6). Rules 1-3 of the request machine read `house_has`, never "for anyone" (DC2 wires them). Red first on
    /// the waiting upload (today's had rule takes it for the house); the pins `only_arrived_music_counts` (room.rs) and
    /// `two_copies_never_cover_each_other` (backup_list.rs) stay as they are, in their crates.
    #[test]
    fn the_house_has_it_by_its_own_rule() {
        let only_b = [own(OWN_B, "bob", 10)];
        assert!(house_has(&only_b, Some(10)), "Soulseek's RecordHad and the round's copy for an asker read it");
        assert_eq!(of_ten(&only_b, "alice"), Holding::Elsewhere, "while alice's holding is Elsewhere");
        assert!(!house_has(&[upload("bob", 10)], Some(10)), "never by a waiting upload");
        assert!(house_has(&[incoming(IN, 10)], Some(10)), "an arrival from a fetcher is the house's");
        assert!(!house_has(&[main_copy(MUSIC, 7)], Some(10)), "partly is not had");
        assert!(!house_has(&[gone(MORE, T)], Some(10)));
    }
}
