//! What an import decides of one item from what the server says of it, moved down from import.rs for Last.fm's import (listening):
//! the rules of an import again, against what the last one brought in (`userdata::Imported`).

use crate::userdata::Imported;

/// What the server says of one item now; a part not read is `None`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Seen {
    pub server_item: String,
    /// Plays and last played.
    pub plays: Option<(u32, Option<i64>)>,
    /// Favourite, and since when if the server keeps that.
    pub star: Option<(bool, Option<i64>)>,
    pub rating: Option<u8>,
}

/// What to change for one item, and what to keep for the next import.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Decision {
    pub add_plays: u32,
    pub played: Option<i64>,
    pub star: Option<(bool, Option<i64>)>,
    pub rating: Option<u8>,
    pub now: Imported,
}

impl Decision {
    /// Nothing to write: no change, and the record would stay as it is.
    pub fn is_nothing(&self, last: Option<&Imported>) -> bool {
        self.add_plays == 0 && self.played.is_none() && self.star.is_none() && self.rating.is_none() && last == Some(&self.now)
    }
}

/// The rules of an import again (see the module): the server's values against what the last import brought in.
pub fn decide(last: Option<&Imported>, seen: &Seen) -> Decision {
    let before = last.cloned().unwrap_or_default();
    let mut d = Decision { now: Imported { server_item: Some(seen.server_item.clone()), ..before.clone() }, ..Decision::default() };
    if let Some((plays, played)) = seen.plays {
        // Fewer than last time (the count was reset on the server): nothing to add, and later plays count from there.
        d.add_plays = plays.saturating_sub(before.plays);
        if played.is_some() && played != before.played {
            d.played = played;
        }
        d.now.plays = plays;
        d.now.played = played;
    }
    if let Some((on, at)) = seen.star {
        if on != before.starred {
            d.star = Some((on, at));
        }
        d.now.starred = on;
    }
    if let Some(r) = seen.rating {
        if r != before.rating {
            d.rating = Some(r);
        }
        d.now.rating = r;
    }
    d
}
