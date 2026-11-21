//! The wording table's tests (SCENARIOS-DC2 SD1, SD2, SD4, SD5, SD7, SD8, PS1, PS3).

use super::*;
use crate::request::Handed;
use crate::soulseek::{Picker, Source, SourceKey};

const T: Time = 1_800_000_000;

fn open() -> Request {
    Request::Open { handed: Handed::default() }
}

fn of<'a>(request: &'a Request, item: Option<&'a Item>) -> Of<'a> {
    Of { request, held: Held::NotHad, wait: None, release_day: None, lidarr: None, item, soulseek_on: true, busy_user: None, now: T }
}

fn item(state: State) -> Item {
    Item { sources: vec![Source { key: SourceKey::new("peer-a", "d"), certain: false }], state, ..Item::new(11) }
}

fn fetching(percent: u8) -> State {
    State::Fetching { source: SourceKey::new("peer-a", "d"), by: Picker::Cratarr, percent }
}

/// Every standing, one of each.
fn every() -> Vec<Standing> {
    let mut out = vec![
        Standing::Delivered { at: T },
        Standing::Rejected { at: T },
        Standing::TakenBack { at: T },
        Standing::AskedTwice { at: T },
        Standing::Merged { at: T, into: 20 },
        Standing::Declined { at: T },
        Standing::Waits { wait: Wait::Room, day: None },
        Standing::Waits { wait: Wait::Space, day: None },
        Standing::Waits { wait: Wait::Release, day: Some(T + 86_400) },
        Standing::Done,
        Standing::Arrived,
        Standing::Partly { has: 7, of: 10 },
        Standing::SwitchedOff { source: "Soulseek" },
        Standing::OnItsWay,
        Standing::Fetching { percent: 45 },
        Standing::Fetched,
        Standing::Checking { done: 4, of: 9 },
        Standing::Placing,
        Standing::WaitsForUser { user: "peer-a".into() },
        Standing::WaitsForPick,
        Standing::Looking,
        Standing::NotFoundYet { again: T + 3600 },
        Standing::NotAnswering { source: "Lidarr" },
        Standing::Stuck { why: "No list to judge by".into() },
        Standing::Asked,
    ];
    for hold in [
        Hold::Paused { source: "acoustid".into(), until: T + 120 },
        Hold::SlskdAway { said: "slskd could not be reached".into() },
        Hold::Floor,
        Hold::Room,
        Hold::AcoustidAway { said: "AcoustID answered 503".into() },
        Hold::PlaceFailed { tries: 1, until: T + 900 },
        Hold::LocalFailures { until: T + 86_400 },
        Hold::SlskdOff,
        Hold::BackgroundPaused,
        Hold::Other { said: "an error's own words".into() },
    ] {
        out.push(Standing::Held { hold });
    }
    out
}

/// SD1: one standing, one wording table, every state for every audience: each has its words, a closed one its date; administrators
/// also read a source's own words, others never; one state said once per line (SB20).
#[test]
fn every_request_state_has_its_words_for_everyone() {
    for s in every() {
        for audience in [Audience::Asker, Audience::Other, Audience::Administrator] {
            let shown = shown(&s, audience, Some("Lidarr is looking for it"));
            assert!(!shown.words.is_empty() && !shown.words.contains('{'), "{s:?} for {audience:?}: {shown:?}");
            assert_eq!(shown.said.is_some(), audience == Audience::Administrator, "{s:?}: only administrators read a source's own words");
            if let Some(said) = &shown.said {
                assert!(!shown.words.contains(said.as_str()), "{s:?}: said once in a line");
            }
        }
    }
    for s in &every()[..6] {
        assert!(shown(s, Audience::Asker, None).at.is_some(), "{s:?}: closed with its date");
    }
    for (key, _) in WORDS {
        assert!(WORDS.iter().filter(|(k, _)| *k == key).count() == 1, "{key} once");
    }
}

/// SD2: the standing's order (C.5), the first that holds: closed with its date; a wait worked out now; the holding; a fetcher
/// switched off holding the only part; the furthest fetch, or held; what waits for someone; looking or not found yet; Stuck, where
/// a Lidarr still looking speaks first. Never "looking" before what waits for someone, nor Stuck over a Lidarr still looking.
#[test]
fn the_standing_takes_the_first_that_holds() {
    let closed = Request::Closed { how: How::TakenBack, at: T, by: Some("alice".into()), reason: None };
    let r = open();
    let found = item(State::Found);
    let stuck = item(State::Stuck { why: "No list to judge by".into(), due: T + 86_400 });
    let (f60, waiting) = (item(fetching(60)), item(State::Waiting { due: T + 3600 }));
    let rows: Vec<(Of, Standing)> = vec![
        (Of { wait: Some(Wait::Room), ..of(&closed, Some(&found)) }, Standing::TakenBack { at: T }),
        (Of { wait: Some(Wait::Room), held: Held::Partly { has: 3, of: 8 }, ..of(&r, Some(&found)) }, Standing::Waits { wait: Wait::Room, day: None }),
        (Of { wait: Some(Wait::Release), release_day: Some(T + 7 * 86_400), ..of(&r, None) }, Standing::Waits { wait: Wait::Release, day: Some(T + 7 * 86_400) }),
        (Of { held: Held::OthersOwn, ..of(&r, Some(&found)) }, Standing::Arrived),
        (Of { held: Held::Partly { has: 3, of: 8 }, ..of(&r, Some(&found)) }, Standing::Partly { has: 3, of: 8 }),
        (Of { lidarr: Some(LidarrNow::Fetching { percent: 20 }), ..of(&r, Some(&f60)) }, Standing::Fetching { percent: 60 }),
        (Of { lidarr: Some(LidarrNow::Fetching { percent: 80 }), ..of(&r, Some(&f60)) }, Standing::Fetching { percent: 80 }),
        (Of { lidarr: Some(LidarrNow::Looking), ..of(&r, Some(&found)) }, Standing::WaitsForPick),
        (Of { lidarr: Some(LidarrNow::Looking), ..of(&r, Some(&stuck)) }, Standing::Looking),
        (of(&r, Some(&stuck)), Standing::Stuck { why: "No list to judge by".into() }),
        (of(&r, Some(&waiting)), Standing::NotFoundYet { again: T + 3600 }),
        (of(&r, None), Standing::Asked),
    ];
    for (i, (of, want)) in rows.iter().enumerate() {
        assert_eq!(&standing(of), want, "row {i}");
    }
}

/// SD4: Soulseek switched off, holding the only part, says so: never a frozen "Found on Soulseek" or "Fetching · 10%".
#[test]
fn a_switched_off_soulseek_says_so() {
    let r = open();
    for i in [item(State::Found), item(fetching(10))] {
        let s = standing(&Of { soulseek_on: false, ..of(&r, Some(&i)) });
        assert_eq!(s, Standing::SwitchedOff { source: "Soulseek" });
        assert_eq!(shown(&s, Audience::Asker, None).words, "Soulseek is switched off");
    }
    let lidarrs = standing(&Of { soulseek_on: false, lidarr: Some(LidarrNow::Looking), ..of(&r, Some(&item(State::Found))) });
    assert_ne!(lidarrs, Standing::SwitchedOff { source: "Soulseek" }, "not the only part");
}

/// SD5: the top line says what Lidarr's line says: Lidarr not answering, or not at it; never stored words.
#[test]
fn the_top_line_says_what_lidarrs_line_says() {
    let r = open();
    let s = standing(&Of { lidarr: Some(LidarrNow::NotAnswering), ..of(&r, None) });
    assert_eq!(shown(&s, Audience::Asker, None).words, "Lidarr isn't answering");
    assert_eq!(standing(&Of { lidarr: Some(LidarrNow::NotAtIt), ..of(&r, None) }), Standing::Asked, "not at it: nothing of Lidarr's");
}

/// SD7: Found waits for a pick, in its words.
#[test]
fn found_waits_for_a_pick() {
    let r = open();
    let s = standing(&of(&r, Some(&item(State::Found))));
    assert_eq!(shown(&s, Audience::Asker, None).words, "Found: waits for a pick");
}

/// SD8: the list and the request word an item alike: a pick waiting for its user's other record; one held by the floor says the
/// server is short of space in both, never its user's other record (L6); the request below the floor whose item the floor holds
/// says the item's words, not a second "Waits for space" (the floor's race between two layers, 2026-11-21).
#[test]
fn the_list_and_the_request_word_an_item_alike() {
    let r = open();
    let picked = item(State::Picked { source: SourceKey::new("peer-a", "d"), by: Picker::Cratarr, at: T });
    let busy = Of { busy_user: Some("peer-a"), ..of(&r, Some(&picked)) };
    assert_eq!(standing(&busy), item_standing(&picked, Some("peer-a"), T));
    assert_eq!(shown(&standing(&busy), Audience::Asker, None).words, "Waiting for peer-a's other record");
    let floored = Item { held: Some(Hold::Floor), ..picked.clone() };
    let below = Of { wait: Some(Wait::Space), busy_user: Some("peer-a"), ..of(&r, Some(&floored)) };
    assert_eq!(standing(&below), Standing::Held { hold: Hold::Floor });
    assert_eq!(item_standing(&floored, Some("peer-a"), T), Standing::Held { hold: Hold::Floor }, "the list says the same");
    assert_eq!(shown(&standing(&below), Audience::Asker, None).words, "The server is short of space");
    assert_eq!(standing(&Of { wait: Some(Wait::Space), ..of(&r, None) }), Standing::Waits { wait: Wait::Space, day: None }, "not handed: waits for space");
}

/// PS1, PS3: a hold with a deadline says "Paused until" with its time for the page's `when()`; one with none says its own words and
/// no time; a past deadline holds no more; administrators read the pausing source's words beside it, the asker never.
#[test]
fn a_pause_says_until_when() {
    let r = open();
    let paused = Item { held: Some(Hold::Paused { source: "acoustid".into(), until: T + 300 }), ..item(State::Fetched { source: SourceKey::new("peer-a", "d"), by: Picker::Cratarr }) };
    let s = standing(&of(&r, Some(&paused)));
    let asker = shown(&s, Audience::Asker, None);
    assert_eq!((asker.words.as_str(), asker.at, asker.said), ("Paused until", Some(T + 300), None));
    let admin = shown(&s, Audience::Administrator, None);
    assert_eq!(admin.said.as_deref(), Some("AcoustID asked Cratarr to wait"));
    let past = standing(&Of { now: T + 301, ..of(&r, Some(&paused)) });
    assert_eq!(shown(&past, Audience::Asker, None).words, "Waits to be checked", "the line ends when the pause does");
    for (hold, words) in [(Hold::SlskdOff, "slskd is switched off"), (Hold::BackgroundPaused, "Background reading is paused"), (Hold::Floor, "The server is short of space")] {
        let s = shown(&Standing::Held { hold }, Audience::Asker, None);
        assert_eq!((s.words.as_str(), s.at), (words, None), "no deadline, no time");
    }
}
