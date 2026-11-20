//! Where a record is seen from, the when-table's shell (DOMAIN-CORE §7; SCENARIOS-DC1 RS1, HD7): the places a sheet opens
//! from, and the libraries each counts. Every list from the libraries follows the one app-wide switch (the owner, 2026-11-20
//! 21:37 and 23:14; SCENARIOS-DC1b LC10, LC11, CU20.3, which win over HD7's first rows): the Library, an artist's or a label's
//! page, global search and a playlist the switch; a crate its own library rows, then the switch narrows them (a fixed crate,
//! Home's two included, draws from every library first). What is no list of the libraries counts every library: a request
//! never follows a viewer's switch (A3.1.3). Each place's sections, their order and its context are RS1's (DC1a), added here
//! as their rows come.

use cratarr_rules::holding::On;

/// A place a record's sheet or page opens from (RS1's `from`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// The Library and its page search, Home's cards, More from, Similar, Spotify's library.
    Library,
    /// An artist's "In your library" and "Not in your library".
    Artist,
    /// Global search's Records, Home's search.
    Search,
    /// A work's sheet.
    Work,
    /// A label page, Beyond.
    Missing,
    /// Discover's card.
    Discover,
    /// A crate's records row, crate results: `without`, the libraries its own rows leave out (none for a fixed crate, Home's two
    /// included).
    Crate { without: Vec<i64> },
    /// A playlist's flow: its lines never change; what shows follows the switch (LC11).
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

/// The libraries a place counts (HD7, LC10): `switched_off` is the viewer's switch (the device's own, on the device).
pub fn library_set(place: &Place, switched_off: &[i64]) -> On {
    match place {
        Place::Library | Place::Artist | Place::Missing | Place::Search | Place::Playlist | Place::Device => On::without(switched_off),
        // Its own rows first, then the switch narrows them.
        Place::Crate { without } => {
            let mut off = without.clone();
            off.extend(switched_off.iter().filter(|f| !without.contains(f)));
            On::without(&off)
        }
        Place::Several { opener } => library_set(opener, switched_off),
        Place::Page { from } => library_set(from, switched_off),
        Place::Work | Place::Discover | Place::Player | Place::Downloads | Place::Incoming | Place::Problem | Place::Requests | Place::Soulseek => {
            On::all()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cratarr_rules::holding::{CopyFacts, Holding, Library, holding};

    const MUSIC: i64 = 0;

    /// HD7 (H3), as the owner's 21:37 and 23:14 answers read it (its note; SCENARIOS-DC1b LC10, LC11, CU20.3 win): every list
    /// from the libraries follows the one switch. Red first: today the page's openers send `without` each their own way
    /// (`Library::folders_off`, app/library.rs:590; a crate's `withoutFor`, none for a playlist or Home's crates), and no record's
    /// state line follows the place it sits in.
    #[test]
    fn the_holding_follows_the_places_library_set() {
        const MORE: i64 = 4;
        // Night Ferry whole only in MUSIC; alice has MUSIC switched off.
        let night_ferry = [CopyFacts { folder: MUSIC, library: Library::Main, present: 10, paired: 10, went: None }];
        let switch = [MUSIC];
        let seen_from = |place: &Place| holding(&night_ferry, Some(10), "alice", &library_set(place, &switch));
        // The Library, its search and artist pages, global search, a label's page, a playlist: the switch. "Not in your
        // library", and the artist page lists it there.
        assert_eq!(library_set(&Place::Library, &switch), On::without(&[MUSIC]));
        for place in [Place::Library, Place::Artist, Place::Search, Place::Missing, Place::Playlist] {
            assert_eq!(seen_from(&place), Holding::NotHad, "{place:?}");
        }
        // A crate: its own rows, then the switch narrows them (a fixed crate, Home's two included, from every library first).
        assert_eq!(library_set(&Place::Crate { without: Vec::new() }, &switch), On::without(&[MUSIC]), "a fixed crate");
        assert_eq!(library_set(&Place::Crate { without: vec![MORE] }, &switch), On::without(&[MORE, MUSIC]), "its rows, then the switch");
        assert_eq!(library_set(&Place::Crate { without: vec![MUSIC] }, &switch), On::without(&[MUSIC]), "never widened by the switch");
        assert_eq!(library_set(&Place::Crate { without: vec![MORE] }, &[]), On::without(&[MORE]), "the switch all on: its own rows");
        // Narrowed, never widened: a library the crate's rows name (MUSIC) and the switch has off never shows inside it.
        let rows_name_music = Place::Crate { without: vec![MORE] };
        assert!(!library_set(&rows_name_music, &switch).counts(MUSIC), "its rows name MUSIC, the switch has it off");
        assert_eq!(seen_from(&rows_name_music), Holding::NotHad);
        assert_eq!(seen_from(&Place::Crate { without: Vec::new() }), Holding::NotHad);
        // The select bar and the record's page take their opener's.
        assert_eq!(seen_from(&Place::Several { opener: Box::new(Place::Artist) }), Holding::NotHad);
        assert_eq!(seen_from(&Place::Page { from: Box::new(Place::Search) }), Holding::NotHad);
        // With the switch all on: "In your library".
        assert_eq!(holding(&night_ferry, Some(10), "alice", &library_set(&Place::Playlist, &[])), Holding::Had);
        // The requests' holding never follows a viewer's switch (A3.1.3): alice's Want of it reads "In your library".
        assert_eq!(library_set(&Place::Requests, &switch), On::all());
        assert_eq!(seen_from(&Place::Requests), Holding::Had);
        // Never a state line against the list it sits in: the set a place counts is the one its list counts.
        for place in [Place::Library, Place::Artist, Place::Search, Place::Playlist, Place::Crate { without: Vec::new() }, Place::Requests] {
            let on = library_set(&place, &switch);
            assert_eq!(on.counts(MUSIC), seen_from(&place) == Holding::Had, "{place:?}");
        }
    }
}
