//! `requests`' marks, in base: an open request's mark on an album not in the library, and the marks by record and by what
//! names a record, as records' lists show them. `requests` re-exports them and makes them. And a request as the pages show it
//! (`View`, its state and its album's), which playing's album view carries.

use std::collections::HashMap;

use serde::Serialize;

use crate::userdata::Request;

/// An open request's mark on an album not in the library (0.9.5, the owner: a clock while it is wanted, an arrow in
/// the progress ring while it comes; none once it is in): `wanted`, or `downloading` and how far (0 to 1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Mark {
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done: Option<f32>,
}

/// The open requests' marks (C49b): by the record each asks for (merges followed), and by what names that record for lists
/// that have no record id (Discover's cards, Spotify's albums, search's "beyond"): its release groups, its names (`requested`)
/// and the key it was asked by (Discover's). One coming wins over one only wanted.
#[derive(Debug, Clone, Default)]
pub struct Marks {
    pub by_record: HashMap<i64, Mark>,
    pub by_key: HashMap<String, Mark>,
}

impl Marks {
    /// A record's mark, by the id it answers by now (`Catalogue::record` follows a merge).
    pub fn of_record(&self, record: i64) -> Option<Mark> {
        self.by_record.get(&record).copied()
    }
}

/// An album's key by its names: its artist's and its title's.
pub fn requested(artist: &str, album: &str) -> String {
    format!("{}|{}", super::import::key(artist), super::import::key(album))
}

/// An album's mark where a list has no record id for it: by its release group, else by its artist's and its name's key (a
/// Bandcamp album's Want), as `marks_of` keys them.
pub fn mark_of(marks: &Marks, release_group: Option<&str>, artist: &str, album: &str) -> Option<Mark> {
    release_group.filter(|g| !g.is_empty()).and_then(|g| marks.by_key.get(g)).or_else(|| marks.by_key.get(&requested(artist, album))).copied()
}

/// An album's own state beside its request's (decision 0070): asked for and not handed over yet, looked for, being
/// fetched (how far), in Incoming, partly there, in the library; or not known to the fetcher (it is not connected,
/// or did not answer).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Album {
    Requested,
    Looking,
    Fetching { done: f32 },
    InIncoming,
    PartlyThere { tracks: u32, of: u32 },
    InLibrary,
    Unknown,
}

/// A request as the pages show it: as kept, its state (W5, `request_state`: the one every view reads), and whether Hand over
/// again is open for it; and its album's state as the page has read it so far (decision 0070), from that one state.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct View {
    #[serde(flatten)]
    pub request: Request,
    pub state: RequestState,
    pub album_state: Album,
    pub hand_over_again: bool,
}

/// A request's state, the one every view reads (W5, G1): its status as kept first (waiting for approval, declined, cancelled,
/// rejected), then how its record is held (W4: done, arrived, partly there), then what the fetchers say of one wanted: Lidarr's
/// progress, and Soulseek's item (searching, found and waiting for the tick, fetching and how far, or waiting and why). A
/// request's `fetched` stays the hand-over's words; its state is worked out at each read.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RequestState {
    WaitingForApproval,
    /// Its asker is over their room (AL02.2): handed to no fetcher and copied from nothing till they are under. Their room's
    /// numbers, in bytes, where a view read them (`views_of`).
    WaitingForRoom {
        used: u64,
        room: u64,
    },
    /// The server is short of space (AL03): a user's Want handed to no fetcher, or told to stop, till there is space.
    WaitingForSpace,
    Declined,
    Cancelled,
    Rejected,
    /// Done in a library for its asker, and closed ((c), AL02.8).
    Delivered,
    Wanted {
        #[serde(skip_serializing_if = "Option::is_none")]
        lidarr: Option<Fetching>,
        #[serde(skip_serializing_if = "Option::is_none")]
        soulseek: Option<super::soulseek_base::Word>,
    },
    PartlyThere {
        has: u32,
        of: u32,
    },
    Arrived,
    Done,
}

/// What Lidarr says of a request it was handed (W5): looking for it, or fetching it and how far (0 to 1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Fetching {
    Looking,
    Fetching { done: f32 },
    /// Its grab failed or stalls (X1): Soulseek's turn.
    Stuck,
}
