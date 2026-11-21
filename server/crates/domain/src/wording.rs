//! The one wording table for requests and Soulseek's items (DOMAIN-CORE §4.5, C.5 and C.6; SCENARIOS-DC2 SD1–SD8, PS1, PS3): what a
//! request stands at ([`standing`], C.5's order, the first that holds) and what every standing says to whom ([`shown`]), an
//! exhaustive `match`, so a state without its words does not compile. An item's own line on Soulseek's list is the same table
//! ([`item_standing`]), so the list and the request say one thing for one item (SD8). The page writes no state's words of its own.
//!
//! A time beside the words (`Shown::at`) is the page's `when()` to say: "Paused until 14:05", "Paused until 21 Nov" (R6).

use serde::Serialize;

use crate::request::{Held, How, Request, Time, Wait};
use crate::soulseek::{Hold, Item, State, held_at};

/// Who reads (W5, W6): the asker, another user, an administrator. Administrators also read Lidarr's and slskd's own words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    Asker,
    Other,
    Administrator,
}

/// Where Lidarr is with a record now, as its carrier read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LidarrNow {
    Looking,
    Fetching { percent: u8 },
    Stuck,
    /// Lidarr asked and not answering (SD5): its line and the top line say so alike.
    NotAnswering,
    /// Lidarr not at it (Unknown, NotWanted, or never handed).
    NotAtIt,
}

/// What a request stands at, the first that holds (C.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "is", rename_all = "snake_case")]
pub enum Standing {
    // 1. Closed, with its date.
    Delivered { at: Time },
    Rejected { at: Time },
    TakenBack { at: Time },
    AskedTwice { at: Time },
    Merged { at: Time, into: i64 },
    Declined { at: Time },
    // 2. A wait worked out now.
    Waits { wait: Wait, #[serde(skip_serializing_if = "Option::is_none")] day: Option<Time> },
    // 3. The holding.
    Done,
    Arrived,
    Partly { has: u32, of: u32 },
    // 4. A fetcher switched off that holds the only part (SB3).
    SwitchedOff { source: &'static str },
    // 5. The furthest fetch, with its fill; held, with why and until.
    OnItsWay,
    Fetching { percent: u8 },
    Fetched,
    Checking { done: u16, of: u16 },
    Placing,
    Held { hold: Hold },
    /// A pick waiting for its user's other record (one download per user, SD8).
    WaitsForUser { user: String },
    // 6. What waits for someone: Found, no source certain (F20).
    WaitsForPick,
    // 7. Looking, or not found yet.
    Looking,
    NotFoundYet { again: Time },
    /// A fetcher asked and not answering (SD5).
    NotAnswering { source: &'static str },
    // 8. Stuck: Soulseek's section says why.
    Stuck { why: String },
    /// Open, nothing said by any fetcher yet.
    Asked,
}

/// What a request's standing is worked out from (C.5): the request, how its record is held for its asker, the wait worked out now
/// (`request::waits`) and its release day, Lidarr's word, the record's Soulseek item, whether Soulseek is switched on, a busy user
/// its pick waits for, and now.
pub struct Of<'a> {
    pub request: &'a Request,
    pub held: Held,
    pub wait: Option<Wait>,
    pub release_day: Option<Time>,
    pub lidarr: Option<LidarrNow>,
    pub item: Option<&'a Item>,
    pub soulseek_on: bool,
    pub busy_user: Option<&'a str>,
    pub now: Time,
}

/// What a request stands at (C.5, SD2): the first that holds. A wait comes before the holding's part-way states; the furthest fetch
/// before what waits for someone; what waits for someone before "looking"; a Lidarr still looking before Soulseek's Stuck. Pure.
pub fn standing(of: &Of) -> Standing {
    // 1. Closed, with its date.
    if let Request::Closed { how, at, .. } = of.request {
        let at = *at;
        return match how {
            How::Delivered => Standing::Delivered { at },
            How::Rejected => Standing::Rejected { at },
            How::TakenBack => Standing::TakenBack { at },
            How::AskedTwice => Standing::AskedTwice { at },
            How::Merged { into } => Standing::Merged { at, into: *into },
            How::Declined => Standing::Declined { at },
        };
    }
    let item = of.item;
    let item_held = item.and_then(|i| held_at(i, of.now));
    // 2. A wait worked out now. One whose Soulseek item the floor holds says the item's words (SD8: the list and the request say one
    // thing for one item; the floor's race between the two layers, 2026-11-21).
    if let Some(wait) = of.wait {
        if wait == Wait::Space
            && let Some(Hold::Floor) = item_held
        {
            return Standing::Held { hold: Hold::Floor };
        }
        let day = if wait == Wait::Release { of.release_day } else { None };
        return Standing::Waits { wait, day };
    }
    // 3. The holding (OthersOwn reads "Arrived", OQ-I).
    match of.held {
        Held::Had => return Standing::Done,
        Held::Arrived | Held::OthersOwn => return Standing::Arrived,
        Held::Partly { has, of } => return Standing::Partly { has, of },
        Held::NotHad | Held::Retired => {}
    }
    let lidarr = of.lidarr.clone().filter(|l| *l != LidarrNow::NotAtIt);
    // 4. Soulseek switched off, holding the only part (SB3): never a frozen "Found" or "Fetching · 10%".
    if !of.soulseek_on && item.is_some() && lidarr.is_none() {
        return Standing::SwitchedOff { source: "Soulseek" };
    }
    // 5. The furthest fetch, with its fill; held, with why and until.
    if let Some(i) = item
        && let Some(fetch) = furthest(i, of.busy_user)
    {
        if let Some(hold) = item_held
            && !matches!(fetch, Standing::Placing)
        {
            return Standing::Held { hold: hold.clone() };
        }
        // Lidarr fetching further than Soulseek's download speaks instead.
        if let (Standing::Fetching { percent }, Some(LidarrNow::Fetching { percent: l })) = (&fetch, &lidarr)
            && l > percent
        {
            return Standing::Fetching { percent: *l };
        }
        return fetch;
    }
    if let Some(LidarrNow::Fetching { percent }) = lidarr {
        return Standing::Fetching { percent };
    }
    // 6. What waits for someone: Found, no source certain (F20).
    if let Some(i) = item
        && i.state == State::Found
    {
        return Standing::WaitsForPick;
    }
    // 7. Looking (Lidarr still looking speaks first, before Soulseek's Stuck), or not found yet.
    if matches!(lidarr, Some(LidarrNow::Looking)) || item.is_some_and(|i| i.state == State::Searching) {
        return Standing::Looking;
    }
    // Lidarr asked and not answering (SD5): the top line says so, as Lidarr's line does.
    if let Some(LidarrNow::NotAnswering) = lidarr {
        return Standing::NotAnswering { source: "Lidarr" };
    }
    if let Some(i) = item {
        match &i.state {
            State::Waiting { due } => return Standing::NotFoundYet { again: *due },
            // 8. Stuck.
            State::Stuck { why, .. } => return Standing::Stuck { why: why.clone() },
            _ => {}
        }
    }
    if let Some(LidarrNow::Stuck) = lidarr {
        return Standing::Stuck { why: "Lidarr can't fetch it".into() };
    }
    Standing::Asked
}

/// An item's furthest fetch as a standing, where it is under way.
fn furthest(i: &Item, busy_user: Option<&str>) -> Option<Standing> {
    Some(match &i.state {
        State::Picked { source, .. } if busy_user == Some(source.user.as_str()) => Standing::WaitsForUser { user: source.user.clone() },
        State::Picked { .. } => Standing::OnItsWay,
        State::Fetching { percent, .. } => Standing::Fetching { percent: *percent },
        State::Fetched { .. } => Standing::Fetched,
        State::Checking { done, of, .. } => Standing::Checking { done: *done, of: *of },
        State::Placing => Standing::Placing,
        _ => return None,
    })
}

/// An item's own line on Soulseek's list, from the same table as its asker's request (SD8): its standing as the request would say
/// it for an item alone.
pub fn item_standing(i: &Item, busy_user: Option<&str>, now: Time) -> Standing {
    if i.state == State::Placed {
        return Standing::Arrived;
    }
    let open = Request::Open { handed: Default::default() };
    standing(&Of { request: &open, held: Held::NotHad, wait: None, release_day: None, lidarr: None, item: Some(i), soulseek_on: true, busy_user, now })
}

/// What a standing says: its name, its words with their blanks filled, a time beside them for the page's `when()`, and, for
/// administrators, a source's own words (W6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Shown {
    pub is: &'static str,
    pub words: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<Time>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub said: Option<String>,
}

/// Every standing's words, by its name (SD1): the table `shared/request-states.json` holds. `{…}` blanks are filled from the standing.
pub const WORDS: [(&str, &str); 30] = [
    ("delivered", "Delivered"),
    ("rejected", "Rejected"),
    ("taken_back", "Taken back"),
    ("asked_twice", "Asked twice"),
    ("merged", "Merged into another record"),
    ("declined", "Declined"),
    ("waits_for_room", "Waits for room"),
    ("waits_for_space", "Waits for space"),
    ("waits_for_release", "Waits for its release"),
    ("done", "In your library"),
    ("arrived", "Arrived"),
    ("partly", "Partly there · {has} of {of}"),
    ("switched_off", "{source} is switched off"),
    ("on_its_way", "On its way"),
    ("fetching", "{percent}%"),
    ("fetched", "Waits to be checked"),
    ("checking", "{done} of {of} checked"),
    ("placing", "Being placed"),
    ("paused_until", "Paused until"),
    ("short_of_space", "The server is short of space"),
    ("placing_again", "It could not be placed: tried again"),
    ("cant_store", "Soulseek can't store its files: tried again"),
    ("slskd_off", "slskd is switched off"),
    ("background_off", "Background reading is paused"),
    ("waits_for_user", "Waiting for {user}'s other record"),
    ("waits_for_pick", "Found: waits for a pick"),
    ("looking", "Looking for it"),
    ("not_found_yet", "Not found yet"),
    ("not_answering", "{source} isn't answering"),
    ("asked", "Wanted"),
];

fn words(key: &str) -> &'static str {
    WORDS.iter().find(|(k, _)| *k == key).map(|(_, w)| *w).expect("every key has its words")
}

/// What a standing says to `audience` (SD1, PS1, PS3): every standing has its words for every audience; a hold with a deadline says
/// "Paused until" with its time, one without says its own words and no time (TN.12); an administrator also reads a source's own
/// words (`said`: Lidarr's, slskd's, the pausing source's), never another user (W5). Pure.
pub fn shown(s: &Standing, audience: Audience, said: Option<&str>) -> Shown {
    let mut source_said = said.map(str::to_string);
    let fill = |key: &str, blanks: &[(&str, String)]| {
        let mut w = words(key).to_string();
        for (name, value) in blanks {
            w = w.replace(&format!("{{{name}}}"), value);
        }
        w
    };
    let (is, words, at): (&'static str, String, Option<Time>) = match s {
        Standing::Delivered { at } => ("delivered", fill("delivered", &[]), Some(*at)),
        Standing::Rejected { at } => ("rejected", fill("rejected", &[]), Some(*at)),
        Standing::TakenBack { at } => ("taken_back", fill("taken_back", &[]), Some(*at)),
        Standing::AskedTwice { at } => ("asked_twice", fill("asked_twice", &[]), Some(*at)),
        Standing::Merged { at, .. } => ("merged", fill("merged", &[]), Some(*at)),
        Standing::Declined { at } => ("declined", fill("declined", &[]), Some(*at)),
        Standing::Waits { wait: Wait::Room, .. } => ("waits", fill("waits_for_room", &[]), None),
        Standing::Waits { wait: Wait::Space, .. } => ("waits", fill("waits_for_space", &[]), None),
        Standing::Waits { wait: Wait::Release, day } => ("waits", fill("waits_for_release", &[]), *day),
        Standing::Done => ("done", fill("done", &[]), None),
        Standing::Arrived => ("arrived", fill("arrived", &[]), None),
        Standing::Partly { has, of } => ("partly", fill("partly", &[("has", has.to_string()), ("of", of.to_string())]), None),
        Standing::SwitchedOff { source } => ("switched_off", fill("switched_off", &[("source", source.to_string())]), None),
        Standing::OnItsWay => ("on_its_way", fill("on_its_way", &[]), None),
        Standing::Fetching { percent } => ("fetching", fill("fetching", &[("percent", percent.to_string())]), None),
        Standing::Fetched => ("fetched", fill("fetched", &[]), None),
        Standing::Checking { done, of } => ("checking", fill("checking", &[("done", done.to_string()), ("of", of.to_string())]), None),
        Standing::Placing => ("placing", fill("placing", &[]), None),
        Standing::Held { hold } => {
            let (words, at, own) = hold_words(hold);
            source_said = source_said.or(own);
            ("held", words, at)
        }
        Standing::WaitsForUser { user } => ("waits_for_user", fill("waits_for_user", &[("user", user.clone())]), None),
        Standing::WaitsForPick => ("waits_for_pick", fill("waits_for_pick", &[]), None),
        Standing::Looking => ("looking", fill("looking", &[]), None),
        Standing::NotFoundYet { again } => ("not_found_yet", fill("not_found_yet", &[]), Some(*again).filter(|t| *t > 0)),
        Standing::NotAnswering { source } => ("not_answering", fill("not_answering", &[("source", source.to_string())]), None),
        Standing::Stuck { why } => ("stuck", format!("Stuck: {why}"), None),
        Standing::Asked => ("asked", fill("asked", &[]), None),
    };
    Shown { is, words, at, said: source_said.filter(|_| audience == Audience::Administrator) }
}

/// A hold's words (KM4, PS1): its own, a time where it has a deadline, and a source's own words for administrators.
fn hold_words(h: &Hold) -> (String, Option<Time>, Option<String>) {
    match h {
        Hold::Paused { source, until } => (words("paused_until").to_string(), Some(*until), Some(format!("{} asked Cratarr to wait", name_of(source)))),
        Hold::SlskdAway { said } => (said.clone(), None, None),
        Hold::Floor => (words("short_of_space").to_string(), None, None),
        Hold::Room => (words("waits_for_room").to_string(), None, None),
        Hold::AcoustidAway { said } => (said.clone(), None, None),
        Hold::PlaceFailed { until, .. } => (words("placing_again").to_string(), Some(*until), None),
        Hold::LocalFailures { until } => (words("cant_store").to_string(), Some(*until), None),
        Hold::SlskdOff => (words("slskd_off").to_string(), None, None),
        Hold::BackgroundPaused => (words("background_off").to_string(), None, None),
        Hold::Other { said } => (said.clone(), None, None),
    }
}

/// A source by the name the pages give it.
fn name_of(source: &str) -> &str {
    match source {
        "acoustid" => "AcoustID",
        "musicbrainz" => "MusicBrainz",
        "discogs" => "Discogs",
        "bandcamp" => "Bandcamp",
        "slskd" => "slskd",
        other => other,
    }
}

#[cfg(test)]
mod tests;
