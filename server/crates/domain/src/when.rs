//! Where a record is seen from, the when-table's shell (DOMAIN-CORE §7; SCENARIOS-DC1 RS1, HD7): the places a sheet opens
//! from, and the libraries each counts. Every list from the libraries follows the one app-wide switch (the owner, 2026-11-20
//! 21:37 and 23:14; SCENARIOS-DC1b LC10, LC11, CU20.3, which win over HD7's first rows): the Library, an artist's or a label's
//! page, global search and a playlist the switch; a crate its own library rows, then the switch narrows them (a fixed crate,
//! Home's two included, draws from every library first). What is no list of the libraries counts every library: a request
//! never follows a viewer's switch (A3.1.3).
//!
//! **The when-table** (RS1, decision 0104 point 6): the record sheet's sections in one order, and per place only what it adds:
//! its context, the section that comes right after the head, and its library set ([`WHEN`], written to shared/when.json). What
//! a section holds is the record's and the viewer's (`decide`), never the place's; [`sections`] drops what does not apply and
//! keeps the order. The Play row's order is one too ([`play_order`]): Want last of its large actions, after the preview; from
//! Discover first (RS3, RS11).

use cratarr_rules::holding::On;
use serde::Serialize;

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

/// The record sheet's sections (DOMAIN-CORE §8), in their one order ([`ORDER`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Section {
    /// Cover, title, type, artists, year, the state line: always.
    Head,
    /// The large actions: a copy plays for the viewer, or a preview; Want or Requested.
    Play,
    /// What the place adds: a crate's or a playlist's tracks of it, a work's performers, the selection.
    Context,
    /// The viewer's own request (DC2's).
    Request,
    /// A Soulseek item of it (DC2's).
    Soulseek,
    /// A copy waiting in Incoming, for administrators and whoever asked for it.
    Arrival,
    /// Two versions or more: copies and editions.
    Versions,
    /// A copy the viewer may act on: Save to, Move to, Replace in, Delete, Play from, Cancel conversion.
    Library,
    /// Facts by source (VR01's).
    Sources,
    /// In the app: drawn on the device from its own state.
    Device,
    /// Administrators, where it has issues.
    Health,
    /// From Discover: Hide, Not interested.
    Discover,
    /// Feel, know and go: Favourite, Rating, Share, Web search, Go to, Sounds like this, View.
    Go,
}

/// The sections' one order (RS1).
pub const ORDER: [Section; 13] = [
    Section::Head,
    Section::Play,
    Section::Context,
    Section::Request,
    Section::Soulseek,
    Section::Arrival,
    Section::Versions,
    Section::Library,
    Section::Sources,
    Section::Device,
    Section::Health,
    Section::Discover,
    Section::Go,
];

/// What a place adds as its context (RS1's third column, RS10): none for most.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Context {
    /// A work's sheet: the work, its performers.
    Work,
    /// Discover's own: Hide, Not interested, Not interested in <artist>.
    Discover,
    /// Its tracks in this crate; Not in this crate.
    Crate,
    /// Its tracks here; Remove from this playlist (its owner's).
    Playlist,
    /// The track playing.
    Playing,
    /// The device's copy.
    DeviceCopy,
    /// The review.
    Review,
    /// The problem.
    Problem,
    /// The selection.
    Selection,
    /// The request.
    Request,
}

/// Which libraries a place counts, by its rule (RS1's last column, as [`library_set`] works it out).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Libraries {
    /// The app-wide switch.
    Switch,
    /// The crate's own rows, then the switch narrows them.
    Crate,
    /// Every library.
    All,
    /// What its opener counts.
    Opener,
}

/// One place's row of the when-table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct When {
    /// Its `from` word.
    pub from: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<Context>,
    /// The section right after the head.
    pub first: Section,
    pub libraries: Libraries,
    /// Whether View is offered: never on the record's own page (RS14).
    pub view: bool,
}

const fn row(from: &'static str, context: Option<Context>, first: Section, libraries: Libraries) -> When {
    When { from, context, first, libraries, view: true }
}

/// The when-table (RS1), one row a place, in RS1's order. The device's own music counts the device's switch, as the Library the
/// server's (`library_set`).
pub const WHEN: [When; 17] = [
    row("library", None, Section::Play, Libraries::Switch),
    row("artist", None, Section::Play, Libraries::Switch),
    row("search", None, Section::Play, Libraries::Switch),
    row("work", Some(Context::Work), Section::Play, Libraries::All),
    row("missing", None, Section::Play, Libraries::Switch),
    row("discover", Some(Context::Discover), Section::Play, Libraries::All),
    row("crate", Some(Context::Crate), Section::Context, Libraries::Crate),
    row("playlist", Some(Context::Playlist), Section::Context, Libraries::Switch),
    row("player", Some(Context::Playing), Section::Play, Libraries::All),
    row("downloads", Some(Context::DeviceCopy), Section::Device, Libraries::All),
    row("incoming", Some(Context::Review), Section::Arrival, Libraries::All),
    row("problem", Some(Context::Problem), Section::Health, Libraries::All),
    row("several", Some(Context::Selection), Section::Play, Libraries::Opener),
    row("device", None, Section::Play, Libraries::Switch),
    When { from: "page", context: None, first: Section::Play, libraries: Libraries::Opener, view: false },
    row("requests", Some(Context::Request), Section::Request, Libraries::All),
    row("soulseek", Some(Context::Request), Section::Soulseek, Libraries::All),
];

impl Place {
    /// Its `from` word, the when-table's key.
    pub fn from(&self) -> &'static str {
        match self {
            Place::Library => "library",
            Place::Artist => "artist",
            Place::Search => "search",
            Place::Work => "work",
            Place::Missing => "missing",
            Place::Discover => "discover",
            Place::Crate { .. } => "crate",
            Place::Playlist => "playlist",
            Place::Player => "player",
            Place::Downloads => "downloads",
            Place::Incoming => "incoming",
            Place::Problem => "problem",
            Place::Several { .. } => "several",
            Place::Device => "device",
            Place::Page { .. } => "page",
            Place::Requests => "requests",
            Place::Soulseek => "soulseek",
        }
    }

    /// The place a `from` word names, with what it carries: a crate the libraries its own rows leave out; the select bar and the
    /// record's own page their opener's `from` (`opener`; none, the Library's). None: a word no place has.
    pub fn of(from: &str, crate_without: &[i64], opener: Option<&str>) -> Option<Place> {
        let opened = || Box::new(opener.and_then(|o| Place::of(o, crate_without, None)).unwrap_or(Place::Library));
        Some(match from {
            "library" => Place::Library,
            "artist" => Place::Artist,
            "search" => Place::Search,
            "work" => Place::Work,
            // A label's page lists what it has as the Library does (LC10): the missing place's row.
            "missing" | "label" => Place::Missing,
            "discover" => Place::Discover,
            "crate" => Place::Crate { without: crate_without.to_vec() },
            "playlist" => Place::Playlist,
            "player" => Place::Player,
            "downloads" => Place::Downloads,
            "incoming" => Place::Incoming,
            "problem" => Place::Problem,
            "several" => Place::Several { opener: opened() },
            "device" => Place::Device,
            "page" => Place::Page { from: opened() },
            "requests" => Place::Requests,
            "soulseek" => Place::Soulseek,
            _ => return None,
        })
    }

    /// Its row of the when-table.
    pub fn when(&self) -> &'static When {
        let from = self.from();
        WHEN.iter().find(|w| w.from == from).expect("every place has its row")
    }
}

/// The sections a sheet from `place` shows: the one order, the place's first right after the head, and only those that
/// `applies` says hold something (RS1). The head always.
pub fn sections(place: &Place, applies: impl Fn(Section) -> bool) -> Vec<Section> {
    let first = place.when().first;
    let mut out = vec![Section::Head];
    if first != Section::Head && applies(first) {
        out.push(first);
    }
    out.extend(ORDER.iter().copied().filter(|s| *s != Section::Head && *s != first && applies(*s)));
    out
}

/// The Play row's large actions in their order (decision 0058's A; the record page's header's): Play, Resume, Shuffle, Play
/// next, Add to queue, Start radio, then Want or Requested last; from Discover, Want or Requested first (RS3, RS11). Names as
/// `words::entry` has them.
pub fn play_order(place: &Place) -> Vec<&'static str> {
    const LARGE: [&str; 6] = ["play", "resume", "shuffle", "play_next", "add_to_queue", "start_radio"];
    const WANT: [&str; 2] = ["want", "requested"];
    let from_discover = matches!(place, Place::Discover) || matches!(place, Place::Page { from } if **from == Place::Discover);
    if from_discover { WANT.iter().chain(LARGE.iter()).copied().collect() } else { LARGE.iter().chain(WANT.iter()).copied().collect() }
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

    fn every_place() -> Vec<Place> {
        WHEN.iter().map(|w| Place::of(w.from, &[], Some("artist")).unwrap()).collect()
    }

    /// The when-table's rows agree with `library_set` (HD7), and every place has one row: a `from` word reads back as its place.
    #[test]
    fn every_place_has_its_one_row() {
        for w in WHEN {
            let place = Place::of(w.from, &[], None).unwrap();
            assert_eq!(place.from(), w.from);
            assert_eq!(WHEN.iter().filter(|o| o.from == w.from).count(), 1, "{} twice", w.from);
            let switched = library_set(&place, &[MUSIC]);
            match w.libraries {
                Libraries::Switch => assert_eq!(switched, On::without(&[MUSIC]), "{}", w.from),
                Libraries::All => assert_eq!(switched, On::all(), "{}", w.from),
                Libraries::Crate | Libraries::Opener => {}
            }
        }
        assert_eq!(Place::of("label", &[], None), Some(Place::Missing), "a label's page: the missing place's row");
        assert_eq!(Place::of("nowhere", &[], None), None);
    }

    /// RS3: Want (or Requested) is the Play row's last large action wherever the sheet opens, and from Discover its first; never twice.
    /// Red first: no Play row's order on the server (the record page's header and AlbumMenu's `inOrder` each their own).
    #[test]
    fn want_is_in_the_sheet_wherever_it_opens() {
        for place in every_place() {
            let order = play_order(&place);
            let want = order.iter().position(|n| *n == "want").expect("Want in every place's Play row");
            assert_eq!(order.iter().filter(|n| **n == "want").count(), 1, "{place:?}");
            assert_eq!(order[want + 1..].iter().filter(|n| **n != "requested").count() == 0, place != Place::Discover, "{place:?}: Want last");
        }
        assert_eq!(&play_order(&Place::Discover)[..2], ["want", "requested"], "from Discover Want comes first");
        assert_eq!(&play_order(&Place::Library)[..1], ["play"]);
    }

    /// RS10: a place adds only its context and which section comes first: from a crate the Context first, from a work its performers
    /// after the Play row; the Library adds none. `sections` keeps the one order and drops what holds nothing.
    #[test]
    fn a_place_adds_its_context_and_its_order() {
        let holding = |s: Section| matches!(s, Section::Head | Section::Play | Section::Context | Section::Versions | Section::Library | Section::Go);
        let crate_ = Place::Crate { without: Vec::new() };
        assert_eq!(crate_.when().context, Some(Context::Crate));
        assert_eq!(sections(&crate_, holding), [Section::Head, Section::Context, Section::Play, Section::Versions, Section::Library, Section::Go]);
        assert_eq!(Place::Work.when().context, Some(Context::Work));
        assert_eq!(sections(&Place::Work, holding), [Section::Head, Section::Play, Section::Context, Section::Versions, Section::Library, Section::Go]);
        assert_eq!(Place::Library.when().context, None, "a crate's context never shown from the Library");
        let no_context = |s: Section| s != Section::Context && holding(s);
        assert_eq!(sections(&Place::Library, no_context), [Section::Head, Section::Play, Section::Versions, Section::Library, Section::Go]);
        // Never a section in two places, nor out of the one order but the place's first.
        for place in every_place() {
            let shown = sections(&place, |_| true);
            let mut seen = shown.clone();
            seen.dedup();
            assert_eq!(seen.len(), shown.len(), "{place:?}");
            assert_eq!(shown.len(), ORDER.len());
            let rest: Vec<Section> = shown.iter().copied().filter(|s| *s != place.when().first).collect();
            let mut sorted = rest.clone();
            sorted.sort();
            assert_eq!(rest, sorted, "{place:?}: the one order");
        }
    }

    /// RS11: from Discover, Want first, and Discover's own section (Hide, Not interested, Not interested in <artist>).
    #[test]
    fn from_discover_want_comes_first_with_discovers_own() {
        assert_eq!(Place::Discover.when().context, Some(Context::Discover));
        assert_eq!(play_order(&Place::Discover)[0], "want");
        let shown = sections(&Place::Discover, |s| matches!(s, Section::Head | Section::Play | Section::Discover | Section::Go));
        assert_eq!(shown, [Section::Head, Section::Play, Section::Discover, Section::Go]);
        assert_eq!(play_order(&Place::Page { from: Box::new(Place::Discover) })[0], "want", "its page opened from Discover");
    }

    /// RS14: View on every sheet but the record's own page (no U-turn).
    #[test]
    fn view_is_named_and_never_on_its_own_page() {
        for place in every_place() {
            assert_eq!(place.when().view, !matches!(place, Place::Page { .. }), "{place:?}");
        }
    }
}
