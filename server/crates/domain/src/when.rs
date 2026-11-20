//! Where a record is seen from, the when-table's shell (DOMAIN-CORE §7; SCENARIOS-DC1 RS1, HD7): the places a sheet opens
//! from, and the libraries each counts. A place's library set is what its opener sends as switched off today: the switch
//! for the Library, its search and artist pages (the owner, 2026-11-07), a crate's own for a crate, none elsewhere. Each
//! place's sections, their order and its context are RS1's (DC1a), added here as their rows come.

use cratarr_rules::holding::On;

/// A place a record's sheet or page opens from (RS1's `from`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// The Library and its page search, Home's cards, More from, Similar, Spotify's library.
    Library,
    /// An artist's "In your library" and "Not in your library".
    Artist,
    /// Global search's Records.
    Search,
    /// A work's sheet.
    Work,
    /// A label page, Beyond.
    Missing,
    /// Discover's card.
    Discover,
    /// A crate's records row, crate results: with the libraries the crate draws from.
    Crate { without: Vec<i64> },
    /// A playlist's flow.
    Playlist,
    /// The player, the queue.
    Player,
    /// Downloads.
    Downloads,
    /// Health › Incoming.
    Incoming,
    /// Health › a problem.
    Problem,
    /// The select bar: what its opener counts.
    Several { opener: Box<Place> },
    /// This device's music: the device's own set.
    Device,
    /// The record's own page: the place it was opened from.
    Page { from: Box<Place> },
    /// Requests.
    Requests,
    /// Soulseek's list.
    Soulseek,
}

/// The libraries a place counts (HD7): `switched_off` is the viewer's switch (the device's own, on the device).
pub fn library_set(place: &Place, switched_off: &[i64]) -> On {
    match place {
        Place::Library | Place::Artist | Place::Device => On::without(switched_off),
        Place::Crate { without } => On::without(without),
        Place::Several { opener } => library_set(opener, switched_off),
        Place::Page { from } => library_set(from, switched_off),
        Place::Search
        | Place::Work
        | Place::Missing
        | Place::Discover
        | Place::Playlist
        | Place::Player
        | Place::Downloads
        | Place::Incoming
        | Place::Problem
        | Place::Requests
        | Place::Soulseek => On::all(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cratarr_rules::holding::{CopyFacts, Holding, Library, holding};

    const MUSIC: i64 = 0;

    /// HD7 (H3). Red first: today the page's openers send `without` each their own way (`Library::folders_off`,
    /// app/library.rs:590; a crate's `withoutFor`), and no record's state line follows the place it sits in.
    #[test]
    fn the_holding_follows_the_places_library_set() {
        // Night Ferry whole only in MUSIC; alice has MUSIC switched off.
        let night_ferry = [CopyFacts { folder: MUSIC, library: Library::Main, present: 10, paired: 10, went: None }];
        let switch = [MUSIC];
        let seen_from = |place: &Place| holding(&night_ferry, Some(10), "alice", &library_set(place, &switch));
        // Places that follow the switch: "Not in your library", and the artist page lists it there.
        assert_eq!(library_set(&Place::Library, &switch), On::without(&[MUSIC]));
        assert_eq!(seen_from(&Place::Library), Holding::NotHad);
        assert_eq!(seen_from(&Place::Artist), Holding::NotHad);
        // Places that don't: global search, a crate whose own libraries include MUSIC, a playlist: "In your library".
        assert_eq!(seen_from(&Place::Search), Holding::Had);
        assert_eq!(seen_from(&Place::Crate { without: Vec::new() }), Holding::Had);
        assert_eq!(seen_from(&Place::Playlist), Holding::Had);
        // A crate's own set, whatever the switch says.
        assert_eq!(seen_from(&Place::Crate { without: vec![MUSIC] }), Holding::NotHad);
        // The select bar and the record's page take their opener's.
        assert_eq!(seen_from(&Place::Several { opener: Box::new(Place::Artist) }), Holding::NotHad);
        assert_eq!(seen_from(&Place::Page { from: Box::new(Place::Search) }), Holding::Had);
        // The requests' holding never follows a viewer's switch (A3.1.3): alice's Want of it reads "In your library".
        assert_eq!(library_set(&Place::Requests, &switch), On::all());
        assert_eq!(seen_from(&Place::Requests), Holding::Had);
        // Never a state line against the list it sits in: the set a place counts is the one its list counts.
        for place in [Place::Library, Place::Artist, Place::Search, Place::Playlist] {
            let on = library_set(&place, &switch);
            assert_eq!(on.counts(MUSIC), seen_from(&place) == Holding::Had, "{place:?}");
        }
    }
}
