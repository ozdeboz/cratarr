//! What an account keeps that a device must hold to answer for it (PREVIEW2's D18, 0101's points 17 and 18): the rest of what a crate
//! and a pick read, beside [`super::Marks`]. The server says it as it stands, whole, as the marks are: it moves with what the account
//! does, weighs little beside the index, and every piece is read the way the server reads it for the same answer. Tracks go by their
//! ids, as a line is a device's to know from the index.

use serde::{Deserialize, Serialize};

use crate::crates::{Condition, Favourites};
use crate::similarity::Signal;

/// One of the account's playlists.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct List {
    /// What a page names it by.
    pub id: String,
    pub name: String,
    pub tracks: Vec<i64>,
}

/// A crate as the account has it: its own, a permanent one, one of Home's, or a fixed one it set something on.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Kept {
    pub id: String,
    pub name: String,
    /// As the server makes them (`crate_def`), its words spelled as the library spells them.
    pub conditions: Vec<Condition>,
    /// A fixed crate's toggles as saved, else all; an own crate's are among its conditions.
    pub favourites: Favourites,
    pub favourite_albums: Favourites,
    /// The tracks it leaves out by hand, its albums' among them, and how many that is as the list says it.
    pub left_out: Vec<i64>,
    pub left: usize,
    /// One of Home's shelves: its tracks as last worked out (the owner, D18: New for you and Grave digging show their last copy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shelf: Option<Vec<i64>>,
    /// On the Crates page; a fixed one the account only set something on is not.
    pub listed: bool,
    pub permanent: bool,
    /// It has a picture of its own.
    pub image: bool,
}

/// What `/api/index/own` says beside the marks.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Own {
    /// The tracks and the records it hearted.
    pub favourites: Vec<i64>,
    pub favourite_albums: Vec<i64>,
    /// The tracks it excluded from every automatic pick, and the artists, by their keys (`excluded_artists`).
    pub excluded: Vec<i64>,
    pub artists_out: Vec<String>,
    pub lists: Vec<List>,
    /// Every account's playlists with nothing of whose or what they are: Radio's "what people put together" (`app::radio`).
    pub together: Vec<Vec<i64>>,
    /// Its Similar by; none: all alike.
    pub similar_by: Option<Vec<Signal>>,
    /// The libraries Radio and Autoplay choose from (More music), by their folders; none: every one.
    pub libraries: Option<Vec<i64>>,
    pub crates: Vec<Kept>,
    /// Its tracks played within the week as its own, each with when (epoch s): what Radio leaves for later.
    pub recent: Vec<(i64, i64)>,
}
