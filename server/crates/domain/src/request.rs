//! A request as one machine (DOMAIN-CORE §4.1 and C.1; SCENARIOS-DC2 SM01): open, with what was handed and to whom, or closed,
//! how, when and by whom. Stored once, typed (SM1): `next()` alone gives a request's new state and what its carrier does.
//!
//! - **Worked out at each read, never stored** (SM3): whether it waits, and for what ([`waits`]): its asker's room, the server's
//!   floor (a user's alone), its release day. "Waits for room" ends the moment an administrator raises the room.
//! - **Per record, not per request** (V2, SM4): Lidarr's want is the record's, over all its open requests ([`lidarr_wanted`]): below
//!   the floor an administrator's request keeps it (AL03.2), the first arrival ends it (X1). The carrier acts on its **edge**
//!   ([`edge`]): wanted to not wanted is a stop, told once; not wanted to wanted is a hand-over.
//! - **The outbox** (SM6): a stop is owed, told, failed and tried again; a stop owed while Lidarr is away survives ([`Lidarr::StopOwed`]).
//! - **System closures are their own** (SM7): "Asked twice" and "Merged" are Cratarr's, never a person's "Taken back".
//!
//! No I/O, no clock: every time is handed in.

use serde::{Deserialize, Serialize};

/// Epoch seconds.
pub type Time = i64;

/// A request's state (C.1): open, with what was handed, or closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "is", rename_all = "snake_case")]
pub enum Request {
    Open {
        handed: Handed,
    },
    Closed {
        how: How,
        at: Time,
        /// Who closed it: none where Cratarr did (Asked twice, Merged, a delivery).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        by: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

/// How a request closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "how", rename_all = "snake_case")]
pub enum How {
    /// Its record done for its asker, at their copy's whole time (F21).
    Delivered,
    /// Its arrival deleted (a Delete command, by `closed_by`'s rule).
    Rejected,
    /// Its asker (or "Manage requests") took it back.
    TakenBack,
    /// The younger of two asked of one record at once (W6): Cratarr's.
    AskedTwice,
    /// The younger of an account's two open requests a merge of records left on one record (C.1): Cratarr's.
    Merged { into: i64 },
    /// 0.16's, kept as history.
    Declined,
}

/// What was handed, and to whom (C.1's `Handed`): replacing 0.17's `fetched` words.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handed {
    pub lidarr: Lidarr,
    /// Handed to Soulseek: its list holds an item of the record for it.
    #[serde(default)]
    pub soulseek: bool,
    #[serde(default)]
    pub house: House,
    /// Lidarr's own words of it, for administrators (W6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub said: Option<String>,
}

/// Where Lidarr is with the request's record.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lidarr {
    /// Not handed to Lidarr (yet).
    #[default]
    NotYet,
    /// Lidarr took it: it looks for it, has it, or fetches it when it comes out.
    Took,
    /// Lidarr refused it (its own words in `said`).
    Refused,
    /// Lidarr could not be reached: handed again once it answers.
    Away,
    /// Told to stop below the floor (AL03.2): handed again once there is space.
    StoppedForSpace,
    /// Told to stop when the record arrived (X1).
    StoppedOnArrival,
    /// A stop owed while Lidarr was away (the outbox): told once it answers.
    StopOwed,
}

impl Lidarr {
    /// Lidarr is at it: told to fetch, and not told to stop since.
    pub fn at_it(self) -> bool {
        self == Lidarr::Took
    }
}

/// The house's copy into the asker's own music (I05, C52).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "house", content = "why", rename_all = "snake_case")]
pub enum House {
    #[default]
    Nothing,
    /// The record is whole only in someone else's own music: a copy owed into the asker's.
    CopyOwed,
    Copied,
    AlreadyHad,
    /// Not made, and why.
    NotCopied(String),
}

/// How the request's record is held for its asker (DOMAIN-CORE §4.3, as the asker's view gives it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Held {
    NotHad,
    Partly { has: u32, of: u32 },
    /// Whole only in Incoming.
    Arrived,
    /// Whole in a main library or the asker's own music.
    Had,
    /// Whole only in someone else's own music (read as "Arrived" by a request, OQ-I).
    OthersOwn,
    /// No present copy, though one was.
    Retired,
}

impl Held {
    /// Whole somewhere the record's fetch is over for (X1: the first arrival ends Lidarr's want).
    pub fn arrived(self) -> bool {
        matches!(self, Held::Arrived | Held::Had | Held::OthersOwn)
    }
}

/// What holds for a request now, worked out at each read (SM3): never stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Now {
    /// The asker is within their room (an administrator always is).
    pub room_ok: bool,
    /// The server is above its floor.
    pub floor_ok: bool,
    /// The asker administers the server: the floor never holds them (AL03.1).
    pub admin: bool,
    /// The record is out: no release day, or its day has come.
    pub released: bool,
    /// The asker may ask now: active, with "Request albums" (RF20: a waiting Want of one who may not stays where it is).
    pub may: bool,
}

/// What a request waits for (SM3), worked out now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Wait {
    Room,
    Space,
    Release,
}

/// What a request waits for now, where it waits (C.5's order: room, space, its release). A closed request, or one whose record is
/// there for its asker, waits for nothing. Pure.
pub fn waits(r: &Request, held: Held, now: &Now) -> Option<Wait> {
    let Request::Open { .. } = r else { return None };
    if matches!(held, Held::Had | Held::Arrived) {
        return None;
    }
    if !now.room_ok && !now.admin {
        return Some(Wait::Room);
    }
    if !now.floor_ok && !now.admin {
        return Some(Wait::Space);
    }
    if !now.released {
        return Some(Wait::Release);
    }
    None
}

/// What happens to a request (C.1's events).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The round looks at it: what holds now, and how its record is held for its asker.
    Round(Now, Held),
    /// Its record turned whole for its asker (Had), nothing holding it back, at the copy's whole time (F21).
    Whole(Time),
    /// Its arrival deleted by a Delete command, by `closed_by`'s rule (C.4).
    ArrivalDeleted { at: Time, by: Option<String> },
    /// Taken back by its asker, or by someone with "Manage requests".
    TakeBack { at: Time, by: String },
    /// A merge of records left it the younger of its asker's two open requests on record `into` (C.1, SM7).
    Merged { into: i64, at: Time },
    /// The younger of two asked of one record at once (W6, SM7).
    AskedTwice { at: Time },
    /// What its hand-over came to (the carrier's answer to `Does::Hand` or `Does::CopyIntoOwn`).
    Handed(Handed),
    /// Lidarr told to stop, or the stop owed (the record's edge, the outbox): Lidarr's part as it is now.
    Lidarr(Lidarr),
    /// The Undo of what closed it (a Delete's, a Keep's): open again, through `ask()` (C.1 check 1).
    Undone,
}

/// What the carrier does after `next()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Does {
    Nothing,
    /// Hand it to the fetchers (Lidarr, Soulseek), one function (SM5): the floor and room already read.
    Hand,
    /// Make the house's copy into its asker's own music (I05), room and floor permitting.
    CopyIntoOwn,
}

/// A request's next state and what its carrier does (C.1's table). A closed request changes only by `Undone` (check 1), and only
/// where a person's act or a delivery closed it: Declined, Asked twice and Merged never open again. Only `Merged` and `AskedTwice`
/// close it as Cratarr's (check 7). Pure.
pub fn next(r: &Request, e: Event) -> (Request, Does) {
    let same = || (r.clone(), Does::Nothing);
    let closed = |how: How, at: Time, by: Option<String>| (Request::Closed { how, at, by, reason: None }, Does::Nothing);
    match (r, e) {
        (Request::Closed { how: How::Delivered | How::Rejected | How::TakenBack, .. }, Event::Undone) => (Request::Open { handed: Handed::default() }, Does::Hand),
        (Request::Closed { .. }, _) => same(),
        (Request::Open { .. }, Event::Undone) => same(),
        (Request::Open { handed }, Event::Round(now, held)) => {
            if !now.may || matches!(held, Held::Had | Held::Arrived) || waits(r, held, &now).is_some() {
                return same();
            }
            if held == Held::OthersOwn {
                return match handed.house {
                    House::Nothing | House::CopyOwed => (Request::Open { handed: Handed { house: House::CopyOwed, ..handed.clone() } }, Does::CopyIntoOwn),
                    _ => same(),
                };
            }
            let handed_to_none = matches!(handed.lidarr, Lidarr::NotYet | Lidarr::StoppedForSpace | Lidarr::Away) && !handed.soulseek;
            if handed_to_none { (r.clone(), Does::Hand) } else { same() }
        }
        (Request::Open { .. }, Event::Whole(at)) => closed(How::Delivered, at, None),
        (Request::Open { .. }, Event::ArrivalDeleted { at, by }) => closed(How::Rejected, at, by),
        (Request::Open { .. }, Event::TakeBack { at, by }) => closed(How::TakenBack, at, Some(by)),
        (Request::Open { .. }, Event::Merged { into, at }) => closed(How::Merged { into }, at, None),
        (Request::Open { .. }, Event::AskedTwice { at }) => closed(How::AskedTwice, at, None),
        (Request::Open { .. }, Event::Handed(handed)) => (Request::Open { handed }, Does::Nothing),
        (Request::Open { handed }, Event::Lidarr(lidarr)) => (Request::Open { handed: Handed { lidarr, ..handed.clone() } }, Does::Nothing),
    }
}

/// A record's open request as Lidarr's want reads it: the request, and whether its asker administers the server.
pub type Asking<'a> = (&'a Request, bool);

/// Whether Lidarr should be at a record (V2, SM4): an open request wants it, not yet arrived (X1: the first arrival ends the want);
/// below the floor only an administrator's keeps it (AL03.2). Per record, never per request. Pure.
pub fn lidarr_wanted(open: &[Asking], held: Held, floor_ok: bool) -> bool {
    !held.arrived() && open.iter().any(|(r, admin)| matches!(r, Request::Open { .. }) && (floor_ok || *admin))
}

/// What the record's want's edge asks of its carrier (C.1): a stop where it turned unwanted with Lidarr at it, a hand-over where it
/// turned wanted. Nothing off an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Stop,
    Hand,
}

/// The edge between a record's want before and after an event, Lidarr at it before (any of its requests `at_it`). Pure.
pub fn edge(before: bool, after: bool, lidarr_at_it: bool) -> Option<Edge> {
    match (before, after) {
        (true, false) if lidarr_at_it => Some(Edge::Stop),
        (false, true) => Some(Edge::Hand),
        _ => None,
    }
}

/// A stop told to Lidarr, as the request's part says it after (the outbox, SM6): told (for space, or on arrival), or owed where
/// Lidarr could not be reached, tried again each round until told. Pure.
pub fn stopped(reached: bool, arrived: bool) -> Lidarr {
    match (reached, arrived) {
        (false, _) => Lidarr::StopOwed,
        (true, true) => Lidarr::StoppedOnArrival,
        (true, false) => Lidarr::StoppedForSpace,
    }
}

#[cfg(test)]
mod tests;
